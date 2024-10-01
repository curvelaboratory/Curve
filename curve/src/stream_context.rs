use crate::consts::{
    CURVE_FC_REQUEST_TIMEOUT_MS, CURVE_ROUTING_HEADER, ARC_FC_CLUSTER, DEFAULT_EMBEDDING_MODEL,
    DEFAULT_INTENT_MODEL, DEFAULT_PROMPT_TARGET_THRESHOLD, GPT_35_TURBO, MODEL_SERVER_NAME,
    RATELIMIT_SELECTOR_HEADER_KEY, SYSTEM_ROLE, USER_ROLE,
};
use crate::filter_context::{embeddings_store, WasmMetrics};
use crate::llm_providers::{LlmProvider, LlmProviders};
use crate::ratelimit::Header;
use crate::stats::IncrementingMetric;
use crate::tokenizer;
use crate::{ratelimit, routing};
use acap::cos;
use http::StatusCode;
use log::{debug, info, warn};
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use public_types::common_types::open_ai::{
    ChatCompletionChunkResponse, ChatCompletionTool, ChatCompletionsRequest,
    ChatCompletionsResponse, FunctionDefinition, FunctionParameter, FunctionParameters, Message,
    ParameterType, StreamOptions, ToolType,
};
use public_types::common_types::{
    EmbeddingType, PromptGuardRequest, PromptGuardResponse, PromptGuardTask,
    ZeroShotClassificationRequest, ZeroShotClassificationResponse,
};
use public_types::configuration::{Overrides, PromptGuards, PromptTarget};
use public_types::embeddings::{
    CreateEmbeddingRequest, CreateEmbeddingRequestInput, CreateEmbeddingResponse,
};
use std::collections::HashMap;
use std::num::NonZero;
use std::rc::Rc;
use std::sync::RwLock;
use std::time::Duration;

enum ResponseHandlerType {
    GetEmbeddings,
    FunctionResolver,
    FunctionCall,
    ZeroShotIntent,
    CurveGuard,
}

pub struct CallContext {
    response_handler_type: ResponseHandlerType,
    user_message: Option<String>,
    prompt_target_name: Option<String>,
    request_body: ChatCompletionsRequest,
    similarity_scores: Option<Vec<(String, f64)>>,
    up_stream_cluster: Option<String>,
    up_stream_cluster_path: Option<String>,
}

pub struct StreamContext {
    pub context_id: u32,
    pub metrics: Rc<WasmMetrics>,
    pub prompt_targets: Rc<RwLock<HashMap<String, PromptTarget>>>,
    pub overrides: Rc<Option<Overrides>>,
    callouts: HashMap<u32, CallContext>,
    ratelimit_selector: Option<Header>,
    streaming_response: bool,
    response_tokens: usize,
    chat_completions_request: bool,
    llm_provider: Option<&'static LlmProvider<'static>>,
    prompt_guards: Rc<Option<PromptGuards>>,
}

impl StreamContext {
    pub fn new(
        context_id: u32,
        metrics: Rc<WasmMetrics>,
        prompt_targets: Rc<RwLock<HashMap<String, PromptTarget>>>,
        prompt_guards: Rc<Option<PromptGuards>>,
        overrides: Rc<Option<Overrides>>,
    ) -> Self {
        StreamContext {
            context_id,
            metrics,
            prompt_targets,
            callouts: HashMap::new(),
            ratelimit_selector: None,
            streaming_response: false,
            response_tokens: 0,
            chat_completions_request: false,
            llm_provider: None,
            prompt_guards,
            overrides,
        }
    }
    fn llm_provider(&self) -> &LlmProvider {
        self.llm_provider
            .expect("the provider should be set when asked for it")
    }

    fn add_routing_header(&mut self) {
        self.add_http_request_header(CURVE_ROUTING_HEADER, self.llm_provider().as_ref());
    }

    fn modify_auth_headers(&mut self) -> Result<(), String> {
        let llm_provider_api_key_value = self
            .get_http_request_header(self.llm_provider().api_key_header())
            .ok_or(format!("missing {} api key", self.llm_provider()))?;

        let authorization_header_value = format!("Bearer {}", llm_provider_api_key_value);

        self.set_http_request_header("Authorization", Some(&authorization_header_value));

        // sanitize passed in api keys
        for provider in LlmProviders::VARIANTS.iter() {
            self.set_http_request_header(provider.api_key_header(), None);
        }

        Ok(())
    }

    fn delete_content_length_header(&mut self) {
        // Remove the Content-Length header because further body manipulations in the gateway logic will invalidate it.
        // Server's generally throw away requests whose body length do not match the Content-Length header.
        // However, a missing Content-Length header is not grounds for bad requests given that intermediary hops could
        // manipulate the body in benign ways e.g., compression.
        self.set_http_request_header("content-length", None);
    }

    fn save_ratelimit_header(&mut self) {
        self.ratelimit_selector = self
            .get_http_request_header(RATELIMIT_SELECTOR_HEADER_KEY)
            .and_then(|key| {
                self.get_http_request_header(&key)
                    .map(|value| Header { key, value })
            });
    }

    fn send_server_error(&self, error: String, override_status_code: Option<StatusCode>) {
        debug!("server error occurred: {}", error);
        self.send_http_response(
            override_status_code
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
                .as_u16()
                .into(),
            vec![],
            Some(error.as_bytes()),
        );
    }

    fn embeddings_handler(&mut self, body: Vec<u8>, mut callout_context: CallContext) {
        let embedding_response: CreateEmbeddingResponse = match serde_json::from_slice(&body) {
            Ok(embedding_response) => embedding_response,
            Err(e) => {
                return self.send_server_error(
                    format!("Error deserializing embedding response: {:?}", e),
                    None,
                );
            }
        };

        let embeddings_vector = &embedding_response.data[0].embedding;

        debug!(
            "embedding model: {}, vector length: {:?}",
            embedding_response.model,
            embeddings_vector.len()
        );

        let prompt_target_embeddings = match embeddings_store().read() {
            Ok(embeddings) => embeddings,
            Err(e) => {
                return self
                    .send_server_error(format!("Error reading embeddings store: {:?}", e), None);
            }
        };

        let prompt_targets = match self.prompt_targets.read() {
            Ok(prompt_targets) => prompt_targets,
            Err(e) => {
                self.send_server_error(format!("Error reading prompt targets: {:?}", e), None);
                return;
            }
        };

        let prompt_target_names = prompt_targets
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        let similarity_scores: Vec<(String, f64)> = prompt_targets
            .iter()
            .map(|(prompt_name, _prompt_target)| {
                let default_embeddings = HashMap::new();
                let pte = prompt_target_embeddings
                    .get(prompt_name)
                    .unwrap_or(&default_embeddings);
                let description_embeddings = pte.get(&EmbeddingType::Description);
                let similarity_score_description = cos::cosine_similarity(
                    &embeddings_vector,
                    &description_embeddings.unwrap_or(&vec![0.0]),
                );
                (prompt_name.clone(), similarity_score_description)
            })
            .collect();

        debug!(
            "similarity scores based on description embeddings match: {:?}",
            similarity_scores
        );

        callout_context.similarity_scores = Some(similarity_scores);

        let zero_shot_classification_request = ZeroShotClassificationRequest {
            // Need to clone into input because user_message is used below.
            input: callout_context.user_message.as_ref().unwrap().clone(),
            model: String::from(DEFAULT_INTENT_MODEL),
            labels: prompt_target_names,
        };

        let json_data: String = match serde_json::to_string(&zero_shot_classification_request) {
            Ok(json_data) => json_data,
            Err(error) => {
                let error = format!("Error serializing zero shot request: {}", error);
                return self.send_server_error(error, None);
            }
        };

        let token_id = match self.dispatch_http_call(
            MODEL_SERVER_NAME,
            vec![
                (":method", "POST"),
                (":path", "/zeroshot"),
                (":authority", MODEL_SERVER_NAME),
                ("content-type", "application/json"),
                ("x-envoy-max-retries", "3"),
                ("x-envoy-upstream-rq-timeout-ms", "60000"),
            ],
            Some(json_data.as_bytes()),
            vec![],
            Duration::from_secs(5),
        ) {
            Ok(token_id) => token_id,
            Err(e) => {
                let error_msg = format!(
                    "Error dispatching embedding server HTTP call for zero-shot-intent-detection: {:?}",
                    e
                );
                return self.send_server_error(error_msg, None);
            }
        };
        debug!(
            "dispatched call to server/zeroshot token_id={}",
            token_id
        );

        self.metrics.active_http_calls.increment(1);
        callout_context.response_handler_type = ResponseHandlerType::ZeroShotIntent;

        if self.callouts.insert(token_id, callout_context).is_some() {
            panic!(
                "duplicate token_id={} in embedding server requests",
                token_id
            )
        }
    }

    fn zero_shot_intent_detection_resp_handler(
        &mut self,
        body: Vec<u8>,
        mut callout_context: CallContext,
    ) {
        let zeroshot_intent_response: ZeroShotClassificationResponse =
            match serde_json::from_slice(&body) {
                Ok(zeroshot_response) => zeroshot_response,
                Err(e) => {
                    self.send_server_error(
                        format!(
                            "Error deserializing zeroshot intent detection response: {:?}",
                            e
                        ),
                        None,
                    );
                    return;
                }
            };

        debug!("zeroshot intent response: {:?}", zeroshot_intent_response);

        let desc_emb_similarity_map: HashMap<String, f64> = callout_context
            .similarity_scores
            .clone()
            .unwrap()
            .into_iter()
            .collect();

        let pred_class_desc_emb_similarity = desc_emb_similarity_map
            .get(&zeroshot_intent_response.predicted_class)
            .unwrap();

        let prompt_target_similarity_score = zeroshot_intent_response.predicted_class_score * 0.7
            + pred_class_desc_emb_similarity * 0.3;

        debug!(
            "similarity score: {:.3}, intent score: {:.3}, description embedding score: {:.3}, prompt: {}",
            prompt_target_similarity_score,
            zeroshot_intent_response.predicted_class_score,
            pred_class_desc_emb_similarity,
            callout_context.user_message.as_ref().unwrap()
        );

        let prompt_target_name = zeroshot_intent_response.predicted_class.clone();

        // Check to see who responded to user message. This will help us identify if control should be passed to Curve FC or not.
        // If the last message was from Curve FC, then Curve FC is handling the conversation (possibly for parameter collection).
        let mut curve _assistant = false;
        let messages = &callout_context.request_body.messages;
        if messages.len() >= 2 {
            let latest_assistant_message = &messages[messages.len() - 2];
            if let Some(model) = latest_assistant_message.model.as_ref() {
                if model.starts_with("Curve") {
                    curve _assistant = true;
                }
            }
        } else {
            info!("no assistant message found, probably first interaction");
        }

        // get prompt target similarity thresold from overrides
        let prompt_target_intent_matching_threshold = match self.overrides.as_ref() {
            Some(overrides) => match overrides.prompt_target_intent_matching_threshold {
                Some(threshold) => threshold,
                None => DEFAULT_PROMPT_TARGET_THRESHOLD,
            },
            None => DEFAULT_PROMPT_TARGET_THRESHOLD,
        };

        // check to ensure that the prompt target similarity score is above the threshold
        if prompt_target_similarity_score < prompt_target_intent_matching_threshold
            && !curve _assistant
        {
            // if curve  fc responded to the user message, then we don't need to check the similarity score
            // it may be that curve  fc is handling the conversation for parameter collection
            if curve _assistant {
                info!("curve  assistant is handling the conversation");
            } else {
                info!(
                    "prompt target below limit: {:.3}, threshold: {:.3}, continue conversation with user",
                    prompt_target_similarity_score,
                    prompt_target_intent_matching_threshold
                );
                self.resume_http_request();
                return;
            }
        }

        let prompt_target = self
            .prompt_targets
            .read()
            .unwrap()
            .get(&prompt_target_name)
            .unwrap()
            .clone();

        info!("prompt_target name: {:?}", prompt_target_name);

        //TODO: handle default function resolver type
        let mut chat_completion_tools: Vec<ChatCompletionTool> = Vec::new();
        for pt in self.prompt_targets.read().unwrap().values() {
            // only extract entity names
            let properties: HashMap<String, FunctionParameter> = match pt.parameters {
                // Clone is unavoidable here because we don't want to move the values out of the prompt target struct.
                Some(ref entities) => {
                    let mut properties: HashMap<String, FunctionParameter> = HashMap::new();
                    for entity in entities.iter() {
                        let param = FunctionParameter {
                            parameter_type: ParameterType::from(
                                entity.parameter_type.clone().unwrap_or("str".to_string()),
                            ),
                            description: entity.description.clone(),
                            required: entity.required,
                            enum_values: entity.enum_values.clone(),
                            default: entity.default.clone(),
                        };
                        properties.insert(entity.name.clone(), param);
                    }
                    properties
                }
                None => HashMap::new(),
            };
            let tools_parameters = FunctionParameters { properties };

            chat_completion_tools.push({
                ChatCompletionTool {
                    tool_type: ToolType::Function,
                    function: FunctionDefinition {
                        name: pt.name.clone(),
                        description: pt.description.clone(),
                        parameters: tools_parameters,
                    },
                }
            });
        }

        let chat_completions = ChatCompletionsRequest {
            model: GPT_35_TURBO.to_string(),
            messages: callout_context.request_body.messages.clone(),
            tools: Some(chat_completion_tools),
            stream: false,
            stream_options: None,
        };

        let msg_body = match serde_json::to_string(&chat_completions) {
            Ok(msg_body) => {
                debug!("curve _fc request body content: {}", msg_body);
                msg_body
            }
            Err(e) => {
                return self
                    .send_server_error(format!("Error serializing request_params: {:?}", e), None);
            }
        };

        let token_id = match self.dispatch_http_call(
            ARC_FC_CLUSTER,
            vec![
                (":method", "POST"),
                (":path", "/v1/chat/completions"),
                (":authority", ARC_FC_CLUSTER),
                ("content-type", "application/json"),
                ("x-envoy-max-retries", "3"),
                (
                    "x-envoy-upstream-rq-timeout-ms",
                    CURVE_FC_REQUEST_TIMEOUT_MS.to_string().as_str(),
                ),
            ],
            Some(msg_body.as_bytes()),
            vec![],
            Duration::from_secs(5),
        ) {
            Ok(token_id) => token_id,
            Err(e) => {
                let error_msg = format!("Error dispatching HTTP call for function-call: {:?}", e);
                return self.send_server_error(error_msg, Some(StatusCode::BAD_REQUEST));
            }
        };

        debug!(
            "dispatched call to function {} token_id={}",
            ARC_FC_CLUSTER, token_id
        );

        self.metrics.active_http_calls.increment(1);
        callout_context.response_handler_type = ResponseHandlerType::FunctionResolver;
        callout_context.prompt_target_name = Some(prompt_target.name);
        if self.callouts.insert(token_id, callout_context).is_some() {
            panic!("duplicate token_id")
        }
    }

    fn function_resolver_handler(&mut self, body: Vec<u8>, mut callout_context: CallContext) {
        debug!("response received for function resolver");

        let body_str = String::from_utf8(body).unwrap();
        debug!("function_resolver response str: {}", body_str);

        let curve _fc_response: ChatCompletionsResponse = match serde_json::from_str(&body_str) {
            Ok(curve _fc_response) => curve _fc_response,
            Err(e) => {
                return self.send_server_error(
                    format!(
                        "Error deserializing function resolver response into ChatCompletion: {:?}",
                        e
                    ),
                    None,
                );
            }
        };

        let model_resp = &curve _fc_response.choices[0];

        if model_resp.message.tool_calls.is_none() {
            // This means that Curve FC did not have enough information to resolve the function call
            // Curve FC probably responded with a message asking for more information.
            // Let's send the response back to the user to initalize lightweight dialog for parameter collection

            //TODO: add resolver name to the response so the client can send the response back to the correct resolver

            return self.send_http_response(
                StatusCode::OK.as_u16().into(),
                vec![("Powered-By", "Katanemo")],
                Some(body_str.as_bytes()),
            );
        }

        let tool_calls = model_resp.message.tool_calls.as_ref().unwrap();
        if tool_calls.is_empty() {
            return self.send_server_error(
                "No tool calls found in function resolver response".to_string(),
                Some(StatusCode::BAD_REQUEST),
            );
        }

        debug!("tool_call_details: {:?}", tool_calls);
        // extract all tool names
        let tool_names: Vec<String> = tool_calls
            .iter()
            .map(|tool_call| tool_call.function.name.clone())
            .collect();

        debug!(
            "call context similarity score: {:?}",
            callout_context.similarity_scores
        );
        //HACK: for now we only support one tool call, we will support multiple tool calls in the future
        let tool_params = &tool_calls[0].function.arguments;
        let tools_call_name = tool_calls[0].function.name.clone();
        let tool_params_json_str = serde_json::to_string(&tool_params).unwrap();

        let prompt_target = self
            .prompt_targets
            .read()
            .unwrap()
            .get(&tools_call_name)
            .unwrap()
            .clone();

        debug!("prompt_target_name: {}", prompt_target.name);
        debug!("tool_name(s): {:?}", tool_names);
        debug!("tool_params: {}", tool_params_json_str);

        let endpoint = prompt_target.endpoint.unwrap();
        let mut path = endpoint.path.unwrap_or(String::from("/"));
        let method = endpoint
            .method
            .unwrap_or(public_types::configuration::Method::Post);
        let mut body = Some(tool_params_json_str.as_bytes());
        if method == public_types::configuration::Method::Post {
            let mut query_params = vec![];
            for (key, value) in tool_params {
                query_params.push(format!("{}={}", key, format!("{:?}", value)));
            }
            let path_args = &query_params.join("&");
            path.push_str("?");
            path.push_str(path_args);
        } else {
            body = None;
        }
        let token_id = match self.dispatch_http_call(
            &endpoint.name,
            vec![
                (":method", method.to_string().as_str()),
                (":path", path.as_ref()),
                (":authority", endpoint.name.as_str()),
                ("content-type", "application/json"),
                ("x-envoy-max-retries", "3"),
            ],
            body,
            vec![],
            Duration::from_secs(5),
        ) {
            Ok(token_id) => token_id,
            Err(e) => {
                let error_msg = format!(
                    "Error dispatching call to cluster: {}, path: {}, err: {:?}",
                    &endpoint.name, path, e
                );
                debug!("{}", error_msg);
                return self.send_server_error(error_msg, Some(StatusCode::BAD_REQUEST));
            }
        };

        callout_context.up_stream_cluster = Some(endpoint.name);
        callout_context.up_stream_cluster_path = Some(path);
        callout_context.response_handler_type = ResponseHandlerType::FunctionCall;
        if self.callouts.insert(token_id, callout_context).is_some() {
            panic!("duplicate token_id")
        }
        self.metrics.active_http_calls.increment(1);
    }

    fn function_call_response_handler(&mut self, body: Vec<u8>, callout_context: CallContext) {
        let headers = self.get_http_call_response_headers();
        if let Some(http_status) = headers.iter().find(|(key, _)| key == ":status") {
            if http_status.1 != StatusCode::OK.as_str() {
                let error_msg = format!(
                    "Error in function call response: cluster: {}, path: {}, status code: {}",
                    callout_context.up_stream_cluster.unwrap(),
                    callout_context.up_stream_cluster_path.unwrap(),
                    http_status.1
                );
                return self.send_server_error(error_msg, Some(StatusCode::BAD_REQUEST));
            }
        } else {
            warn!("http status code not found in api response");
        }
        debug!("response received for function call response");
        let body_str: String = String::from_utf8(body).unwrap();
        debug!("function_call_response response str: {}", body_str);
        let prompt_target_name = callout_context.prompt_target_name.unwrap();
        let prompt_target = self
            .prompt_targets
            .read()
            .unwrap()
            .get(&prompt_target_name)
            .unwrap()
            .clone();

        let mut messages: Vec<Message> = callout_context.request_body.messages.clone();

        // add system prompt
        match prompt_target.system_prompt.as_ref() {
            None => {}
            Some(system_prompt) => {
                let system_prompt_message = Message {
                    role: SYSTEM_ROLE.to_string(),
                    content: Some(system_prompt.clone()),
                    model: None,
                    tool_calls: None,
                };
                messages.push(system_prompt_message);
            }
        }

        // add data from function call response
        messages.push({
            Message {
                role: USER_ROLE.to_string(),
                content: Some(body_str),
                model: None,
                tool_calls: None,
            }
        });

        // add original user prompt
        messages.push({
            Message {
                role: USER_ROLE.to_string(),
                content: Some(callout_context.user_message.unwrap()),
                model: None,
                tool_calls: None,
            }
        });

        let chat_completions_request: ChatCompletionsRequest = ChatCompletionsRequest {
            model: callout_context.request_body.model,
            messages,
            tools: None,
            stream: callout_context.request_body.stream,
            stream_options: callout_context.request_body.stream_options,
        };

        let json_string = match serde_json::to_string(&chat_completions_request) {
            Ok(json_string) => json_string,
            Err(e) => {
                return self
                    .send_server_error(format!("Error serializing request_body: {:?}", e), None);
            }
        };
        debug!(
            "function_calling sending request to openai: msg {}",
            json_string
        );

        // Tokenize and Ratelimit.
        if let Some(selector) = self.ratelimit_selector.take() {
            if let Ok(token_count) =
                tokenizer::token_count(&chat_completions_request.model, &json_string)
            {
                match ratelimit::ratelimits(None).read().unwrap().check_limit(
                    chat_completions_request.model,
                    selector,
                    NonZero::new(token_count as u32).unwrap(),
                ) {
                    Ok(_) => (),
                    Err(err) => {
                        self.send_server_error(
                            format!("Exceeded Ratelimit: {}", err),
                            Some(StatusCode::TOO_MANY_REQUESTS),
                        );
                        self.metrics.ratelimited_rq.increment(1);
                        return;
                    }
                }
            }
        }

        self.set_http_request_body(0, json_string.len(), &json_string.into_bytes());
        self.resume_http_request();
    }

    fn curve _guard_handler(&mut self, body: Vec<u8>, callout_context: CallContext) {
        debug!("response received for curve  guard");
        let prompt_guard_resp: PromptGuardResponse = serde_json::from_slice(&body).unwrap();
        debug!("prompt_guard_resp: {:?}", prompt_guard_resp);

        if prompt_guard_resp.jailbreak_verdict.is_some()
            && prompt_guard_resp.jailbreak_verdict.unwrap()
        {
            //TODO: handle other scenarios like forward to error target
            let default_err = "Jailbreak detected. Please refrain from discussing jailbreaking.";
            let error_msg = match self.prompt_guards.as_ref() {
                Some(prompt_guards) => match prompt_guards
                    .input_guards
                    .get(&public_types::configuration::GuardType::Jailbreak)
                {
                    Some(jailbreak) => match jailbreak.on_exception.as_ref() {
                        Some(on_exception_details) => match on_exception_details.message.as_ref() {
                            Some(error_msg) => error_msg,
                            None => default_err,
                        },
                        None => default_err,
                    },
                    None => default_err,
                },
                None => default_err,
            };

            return self.send_server_error(error_msg.to_string(), Some(StatusCode::BAD_REQUEST));
        }

        self.get_embeddings(callout_context);
    }

    fn get_embeddings(&mut self, callout_context: CallContext) {
        let user_message = callout_context.user_message.unwrap();
        let get_embeddings_input = CreateEmbeddingRequest {
            // Need to clone into input because user_message is used below.
            input: Box::new(CreateEmbeddingRequestInput::String(user_message.clone())),
            model: String::from(DEFAULT_EMBEDDING_MODEL),
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let json_data: String = match serde_json::to_string(&get_embeddings_input) {
            Ok(json_data) => json_data,
            Err(error) => {
                let error_msg = format!("Error serializing embeddings input: {}", error);
                return self.send_server_error(error_msg, None);
            }
        };

        let token_id = match self.dispatch_http_call(
            MODEL_SERVER_NAME,
            vec![
                (":method", "POST"),
                (":path", "/embeddings"),
                (":authority", MODEL_SERVER_NAME),
                ("content-type", "application/json"),
                ("x-envoy-max-retries", "3"),
                ("x-envoy-upstream-rq-timeout-ms", "60000"),
            ],
            Some(json_data.as_bytes()),
            vec![],
            Duration::from_secs(5),
        ) {
            Ok(token_id) => token_id,
            Err(e) => {
                let error_msg = format!("dispatched call to server/embeddings: {:?}", e);
                return self.send_server_error(error_msg, None);
            }
        };
        debug!(
            "dispatched call to server/embeddings token_id={}",
            token_id
        );

        let call_context = CallContext {
            response_handler_type: ResponseHandlerType::GetEmbeddings,
            user_message: Some(user_message),
            prompt_target_name: None,
            request_body: callout_context.request_body,
            similarity_scores: None,
            up_stream_cluster: None,
            up_stream_cluster_path: None,
        };
        if self.callouts.insert(token_id, call_context).is_some() {
            panic!(
                "duplicate token_id={} in embedding server requests",
                token_id
            )
        }
    }
}

// HttpContext is the trait that allows the Rust code to interact with HTTP objects.
impl HttpContext for StreamContext {
    // Envoy's HTTP model is event driven. The WASM ABI has given implementors events to hook onto
    // the lifecycle of the http request and response.
    fn on_http_request_headers(&mut self, _num_headers: usize, _end_of_stream: bool) -> Action {
        let provider_hint = self
            .get_http_request_header("x-curve -deterministic-provider")
            .is_some();
        self.llm_provider = Some(routing::get_llm_provider(provider_hint));

        self.add_routing_header();
        if let Err(error) = self.modify_auth_headers() {
            self.send_server_error(error, Some(StatusCode::BAD_REQUEST));
        }
        self.delete_content_length_header();
        self.save_ratelimit_header();

        debug!(
            "S[{}] req_headers={:?}",
            self.context_id,
            self.get_http_request_headers()
        );

        Action::Continue
    }

    fn on_http_request_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        // Let the client send the gateway all the data before sending to the LLM_provider.
        // TODO: consider a streaming API.
        if !end_of_stream {
            return Action::Pause;
        }

        if body_size == 0 {
            return Action::Continue;
        }

        // Deserialize body into spec.
        // Currently OpenAI API.
        let mut deserialized_body: ChatCompletionsRequest =
            match self.get_http_request_body(0, body_size) {
                Some(body_bytes) => match serde_json::from_slice(&body_bytes) {
                    Ok(deserialized) => deserialized,
                    Err(msg) => {
                        self.send_server_error(
                            format!("Failed to deserialize: {}", msg),
                            Some(StatusCode::BAD_REQUEST),
                        );
                        return Action::Pause;
                    }
                },
                None => {
                    self.send_server_error(
                        format!(
                            "Failed to obtain body bytes even though body_size is {}",
                            body_size
                        ),
                        None,
                    );
                    return Action::Pause;
                }
            };

        // Set the model based on the chosen LLM Provider
        deserialized_body.model = String::from(self.llm_provider().choose_model());

        self.streaming_response = deserialized_body.stream;
        if deserialized_body.stream && deserialized_body.stream_options.is_none() {
            deserialized_body.stream_options = Some(StreamOptions {
                include_usage: true,
            });
        }

        let user_message = match deserialized_body
            .messages
            .last()
            .and_then(|last_message| last_message.content.clone())
        {
            Some(content) => content,
            None => {
                warn!("No messages in the request body");
                return Action::Continue;
            }
        };

        let prompt_guards = match self.prompt_guards.as_ref() {
            Some(prompt_guards) => {
                debug!("prompt guards: {:?}", prompt_guards);
                prompt_guards
            }
            None => {
                let callout_context = CallContext {
                    response_handler_type: ResponseHandlerType::CurveGuard,
                    user_message: Some(user_message),
                    prompt_target_name: None,
                    request_body: deserialized_body,
                    similarity_scores: None,
                    up_stream_cluster: None,
                    up_stream_cluster_path: None,
                };
                self.get_embeddings(callout_context);
                return Action::Pause;
            }
        };

        let prompt_guard_jailbreak_task = prompt_guards
            .input_guards
            .contains_key(&public_types::configuration::GuardType::Jailbreak);
        if !prompt_guard_jailbreak_task {
            info!("Input guards set but no prompt guards were found");
            let callout_context = CallContext {
                response_handler_type: ResponseHandlerType::CurveGuard,
                user_message: Some(user_message),
                prompt_target_name: None,
                request_body: deserialized_body,
                similarity_scores: None,
                up_stream_cluster: None,
                up_stream_cluster_path: None,
            };
            self.get_embeddings(callout_context);
            return Action::Pause;
        }

        let get_prompt_guards_request = PromptGuardRequest {
            input: user_message.clone(),
            task: PromptGuardTask::Jailbreak,
        };

        let json_data: String = match serde_json::to_string(&get_prompt_guards_request) {
            Ok(json_data) => json_data,
            Err(error) => {
                let error_msg = format!("Error serializing prompt guard request: {}", error);
                self.send_server_error(error_msg, None);
                return Action::Pause;
            }
        };

        let token_id = match self.dispatch_http_call(
            MODEL_SERVER_NAME,
            vec![
                (":method", "POST"),
                (":path", "/guard"),
                (":authority", MODEL_SERVER_NAME),
                ("content-type", "application/json"),
                ("x-envoy-max-retries", "3"),
                ("x-envoy-upstream-rq-timeout-ms", "60000"),
            ],
            Some(json_data.as_bytes()),
            vec![],
            Duration::from_secs(5),
        ) {
            Ok(token_id) => token_id,
            Err(e) => {
                let error_msg = format!(
                    "Error dispatching embedding server HTTP call for prompt-guard: {:?}",
                    e
                );
                self.send_server_error(error_msg, None);
                return Action::Pause;
            }
        };

        debug!("dispatched HTTP call to curve _guard token_id={}", token_id);

        let call_context = CallContext {
            response_handler_type: ResponseHandlerType::CurveGuard,
            user_message: Some(user_message),
            prompt_target_name: None,
            request_body: deserialized_body,
            similarity_scores: None,
            up_stream_cluster: None,
            up_stream_cluster_path: None,
        };
        if self.callouts.insert(token_id, call_context).is_some() {
            panic!(
                "duplicate token_id={} in embedding server requests",
                token_id
            )
        }

        self.metrics.active_http_calls.increment(1);

        Action::Pause
    }

    fn on_http_response_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        debug!(
            "recv [S={}] bytes={} end_stream={}",
            self.context_id, body_size, end_of_stream
        );

        if !self.chat_completions_request {
            if let Some(body_str) = self
                .get_http_response_body(0, body_size)
                .and_then(|bytes| String::from_utf8(bytes).ok())
            {
                debug!("recv [S={}] body_str={}", self.context_id, body_str);
            }
            return Action::Continue;
        }

        if !end_of_stream && !self.streaming_response {
            return Action::Pause;
        }

        let body = self
            .get_http_response_body(0, body_size)
            .expect("cant get response body");

        let body_str = String::from_utf8(body).expect("body is not utf-8");

        if self.streaming_response {
            debug!("streaming response");
            let chat_completions_data = match body_str.split_once("data: ") {
                Some((_, chat_completions_data)) => chat_completions_data,
                None => {
                    self.send_server_error(String::from("parsing error in streaming data"), None);
                    return Action::Pause;
                }
            };

            let chat_completions_chunk_response: ChatCompletionChunkResponse =
                match serde_json::from_str(chat_completions_data) {
                    Ok(de) => de,
                    Err(_) => {
                        if chat_completions_data != "[NONE]" {
                            self.send_server_error(
                                String::from("error in streaming response"),
                                None,
                            );
                            return Action::Continue;
                        }
                        return Action::Continue;
                    }
                };

            if let Some(content) = chat_completions_chunk_response
                .choices
                .first()
                .unwrap()
                .delta
                .content
                .as_ref()
            {
                let model = &chat_completions_chunk_response.model;
                let token_count = tokenizer::token_count(model, content).unwrap_or(0);
                self.response_tokens += token_count;
            }
        } else {
            debug!("non streaming response");
            let chat_completions_response: ChatCompletionsResponse =
                match serde_json::from_str(&body_str) {
                    Ok(de) => de,
                    Err(e) => {
                        self.send_server_error(
                            format!(
                                "error in non-streaming response: {}\n response was={}",
                                e, body_str
                            ),
                            None,
                        );
                        return Action::Pause;
                    }
                };

            self.response_tokens += chat_completions_response.usage.completion_tokens;
        }

        debug!(
            "recv [S={}] total_tokens={} end_stream={}",
            self.context_id, self.response_tokens, end_of_stream
        );

        // TODO:: ratelimit based on response tokens.
        Action::Continue
    }
}

impl Context for StreamContext {
    fn on_http_call_response(
        &mut self,
        token_id: u32,
        _num_headers: usize,
        body_size: usize,
        _num_trailers: usize,
    ) {
        let callout_context = self.callouts.remove(&token_id).expect("invalid token_id");
        self.metrics.active_http_calls.increment(-1);

        if let Some(body) = self.get_http_call_response_body(0, body_size) {
            match callout_context.response_handler_type {
                ResponseHandlerType::GetEmbeddings => {
                    self.embeddings_handler(body, callout_context)
                }
                ResponseHandlerType::ZeroShotIntent => {
                    self.zero_shot_intent_detection_resp_handler(body, callout_context)
                }
                ResponseHandlerType::FunctionResolver => {
                    self.function_resolver_handler(body, callout_context)
                }
                ResponseHandlerType::FunctionCall => {
                    self.function_call_response_handler(body, callout_context)
                }
                ResponseHandlerType::CurveGuard => self.curve _guard_handler(body, callout_context),
            }
        } else {
            self.send_server_error(
                String::from("No response body in inline HTTP request"),
                None,
            );
        }
    }
}

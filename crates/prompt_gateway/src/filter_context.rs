use crate::stream_context::StreamContext;
use common::common_types::EmbeddingType;
use common::configuration::{Configuration, Overrides, PromptGuards, PromptTarget};
use common::consts::CURVE_UPSTREAM_HOST_HEADER;
use common::consts::DEFAULT_EMBEDDING_MODEL;
use common::consts::{CURVE_INTERNAL_CLUSTER_NAME, EMBEDDINGS_INTERNAL_HOST};
use common::embeddings::{
    CreateEmbeddingRequest, CreateEmbeddingRequestInput, CreateEmbeddingResponse,
};
use common::http::CallArgs;
use common::http::Client;
use common::stats::Gauge;
use common::stats::IncrementingMetric;
use log::debug;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub struct WasmMetrics {
    pub active_http_calls: Gauge,
}

impl WasmMetrics {
    fn new() -> WasmMetrics {
        WasmMetrics {
            active_http_calls: Gauge::new(String::from("active_http_calls")),
        }
    }
}

pub type EmbeddingTypeMap = HashMap<EmbeddingType, Vec<f64>>;
pub type EmbeddingsStore = HashMap<String, EmbeddingTypeMap>;

#[derive(Debug)]
pub struct FilterCallContext {
    pub prompt_target_name: String,
    pub embedding_type: EmbeddingType,
}

#[derive(Debug)]
pub struct FilterContext {
    metrics: Rc<WasmMetrics>,
    // callouts stores token_id to request mapping that we use during #on_http_call_response to match the response to the request.
    callouts: RefCell<HashMap<u32, FilterCallContext>>,
    overrides: Rc<Option<Overrides>>,
    system_prompt: Rc<Option<String>>,
    prompt_targets: Rc<HashMap<String, PromptTarget>>,
    prompt_guards: Rc<PromptGuards>,
    embeddings_store: Option<Rc<EmbeddingsStore>>,
    temp_embeddings_store: EmbeddingsStore,
}

impl FilterContext {
    pub fn new() -> FilterContext {
        FilterContext {
            callouts: RefCell::new(HashMap::new()),
            metrics: Rc::new(WasmMetrics::new()),
            system_prompt: Rc::new(None),
            prompt_targets: Rc::new(HashMap::new()),
            overrides: Rc::new(None),
            prompt_guards: Rc::new(PromptGuards::default()),
            embeddings_store: Some(Rc::new(HashMap::new())),
            temp_embeddings_store: HashMap::new(),
        }
    }

    fn process_prompt_targets(&self) {
        for values in self.prompt_targets.iter() {
            let prompt_target = values.1;
            self.schedule_embeddings_call(
                &prompt_target.name,
                &prompt_target.description,
                EmbeddingType::Description,
            );
        }
    }

    fn schedule_embeddings_call(
        &self,
        prompt_target_name: &str,
        input: &str,
        embedding_type: EmbeddingType,
    ) {
        let embeddings_input = CreateEmbeddingRequest {
            input: Box::new(CreateEmbeddingRequestInput::String(String::from(input))),
            model: String::from(DEFAULT_EMBEDDING_MODEL),
            encoding_format: None,
            dimensions: None,
            user: None,
        };
        let json_data = serde_json::to_string(&embeddings_input).unwrap();

        let call_args = CallArgs::new(
            CURVE_INTERNAL_CLUSTER_NAME,
            "/embeddings",
            vec![
                (CURVE_UPSTREAM_HOST_HEADER, EMBEDDINGS_INTERNAL_HOST),
                (":method", "POST"),
                (":path", "/embeddings"),
                (":authority", EMBEDDINGS_INTERNAL_HOST),
                ("content-type", "application/json"),
                ("x-envoy-upstream-rq-timeout-ms", "60000"),
            ],
            Some(json_data.as_bytes()),
            vec![],
            Duration::from_secs(60),
        );

        let call_context = crate::filter_context::FilterCallContext {
            prompt_target_name: String::from(prompt_target_name),
            embedding_type,
        };

        if let Err(error) = self.http_call(call_args, call_context) {
            panic!("{error}")
        }
    }

    fn embedding_response_handler(
        &mut self,
        body_size: usize,
        embedding_type: EmbeddingType,
        prompt_target_name: String,
    ) {
        let prompt_target = self
            .prompt_targets
            .get(&prompt_target_name)
            .unwrap_or_else(|| {
                panic!(
                    "Received embeddings response for unknown prompt target name={}",
                    prompt_target_name
                )
            });

        let body = self
            .get_http_call_response_body(0, body_size)
            .expect("No body in response");
        if !body.is_empty() {
            let mut embedding_response: CreateEmbeddingResponse =
                match serde_json::from_slice(&body) {
                    Ok(response) => response,
                    Err(e) => {
                        panic!(
                            "Error deserializing embedding response. body: {:?}: {:?}",
                            String::from_utf8(body).unwrap(),
                            e
                        );
                    }
                };

            let embeddings = embedding_response.data.remove(0).embedding;
            debug!(
                    "Adding embeddings for prompt target name: {:?}, description: {:?}, embedding type: {:?}",
                    prompt_target.name,
                    prompt_target.description,
                    embedding_type
                );

            let entry = self.temp_embeddings_store.entry(prompt_target_name);
            match entry {
                Entry::Occupied(_) => {
                    entry.and_modify(|e| {
                        if let Entry::Vacant(e) = e.entry(embedding_type) {
                            e.insert(embeddings);
                        } else {
                            panic!(
                                "Duplicate {:?} for prompt target with name=\"{}\"",
                                &embedding_type, prompt_target.name
                            )
                        }
                    });
                }
                Entry::Vacant(_) => {
                    entry.or_insert(HashMap::from([(embedding_type, embeddings)]));
                }
            }

            if self.prompt_targets.len() == self.temp_embeddings_store.len() {
                self.embeddings_store =
                    Some(Rc::new(std::mem::take(&mut self.temp_embeddings_store)))
            }
        }
    }
}

impl Client for FilterContext {
    type CallContext = FilterCallContext;

    fn callouts(&self) -> &RefCell<HashMap<u32, Self::CallContext>> {
        &self.callouts
    }

    fn active_http_calls(&self) -> &Gauge {
        &self.metrics.active_http_calls
    }
}

impl Context for FilterContext {
    fn on_http_call_response(
        &mut self,
        token_id: u32,
        _num_headers: usize,
        body_size: usize,
        _num_trailers: usize,
    ) {
        debug!(
            "filter_context: on_http_call_response called with token_id: {:?}",
            token_id
        );
        let callout_data = self
            .callouts
            .borrow_mut()
            .remove(&token_id)
            .expect("invalid token_id");

        self.metrics.active_http_calls.increment(-1);

        self.embedding_response_handler(
            body_size,
            callout_data.embedding_type,
            callout_data.prompt_target_name,
        )
    }
}

// RootContext allows the Rust code to reach into the Envoy Config
impl RootContext for FilterContext {
    fn on_configure(&mut self, _: usize) -> bool {
        let config_bytes = self
            .get_plugin_configuration()
            .expect("Curve config cannot be empty");

        let config: Configuration = match serde_yaml::from_slice(&config_bytes) {
            Ok(config) => config,
            Err(err) => panic!("Invalid curve  config \"{:?}\"", err),
        };

        self.overrides = Rc::new(config.overrides);

        let mut prompt_targets = HashMap::new();
        for pt in config.prompt_targets {
            prompt_targets.insert(pt.name.clone(), pt.clone());
        }
        self.system_prompt = Rc::new(config.system_prompt);
        self.prompt_targets = Rc::new(prompt_targets);

        if let Some(prompt_guards) = config.prompt_guards {
            self.prompt_guards = Rc::new(prompt_guards)
        }

        true
    }

    fn create_http_context(&self, context_id: u32) -> Option<Box<dyn HttpContext>> {
        debug!(
            "||| create_http_context called with context_id: {:?} |||",
            context_id
        );

        let embedding_store = match self.embeddings_store.as_ref() {
            None => return None,
            Some(store) => Some(Rc::clone(store)),
        };
        Some(Box::new(StreamContext::new(
            context_id,
            Rc::clone(&self.metrics),
            Rc::clone(&self.system_prompt),
            Rc::clone(&self.prompt_targets),
            Rc::clone(&self.prompt_guards),
            Rc::clone(&self.overrides),
            embedding_store,
        )))
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn on_vm_start(&mut self, _: usize) -> bool {
        self.set_tick_period(Duration::from_secs(1));
        true
    }

    fn on_tick(&mut self) {
        debug!("starting up curve  filter in mode: prompt gateway mode");
        self.process_prompt_targets();
        self.set_tick_period(Duration::from_secs(0));
    }
}

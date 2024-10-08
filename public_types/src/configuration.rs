use duration_string::DurationString;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Display;
use std::{collections::HashMap, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Overrides {
    pub prompt_target_intent_matching_threshold: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tracing {
    pub sampling_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    pub version: String,
    pub listener: Listener,
    pub endpoints: HashMap<String, Endpoint>,
    pub llm_providers: Vec<LlmProvider>,
    pub overrides: Option<Overrides>,
    pub system_prompt: Option<String>,
    pub prompt_guards: Option<PromptGuards>,
    pub prompt_targets: Vec<PromptTarget>,
    pub error_target: Option<ErrorTargetDetail>,
    pub ratelimits: Option<Vec<Ratelimit>>,
    pub tracing: Option<Tracing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTargetDetail {
    pub endpoint: Option<EndpointDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listener {
    pub address: String,
    pub port: u16,
    pub message_format: MessageFormat,
    // pub connect_timeout: Option<DurationString>,
}

impl Default for Listener {
    fn default() -> Self {
        Listener {
            address: "".to_string(),
            port: 0,
            message_format: MessageFormat::default(),
            // connect_timeout: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum MessageFormat {
    #[serde(rename = "huggingface")]
    #[default]
    Huggingface,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptGuards {
    pub input_guards: HashMap<GuardType, GuardOptions>,
}

impl PromptGuards {
    pub fn jailbreak_on_exception_message(&self) -> Option<&str> {
        self.input_guards
            .get(&GuardType::Jailbreak)?
            .on_exception
            .as_ref()?
            .message
            .as_ref()?
            .as_str()
            .into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum GuardType {
    #[serde(rename = "jailbreak")]
    Jailbreak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardOptions {
    pub on_exception: Option<OnExceptionDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnExceptionDetails {
    pub forward_to_error_target: Option<bool>,
    pub error_handler: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRatelimit {
    pub selector: LlmRatelimitSelector,
    pub limit: Limit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRatelimitSelector {
    pub http_header: Option<RatelimitHeader>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Header {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ratelimit {
    pub model: String,
    pub selector: Header,
    pub limit: Limit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limit {
    pub tokens: u32,
    pub unit: TimeUnit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeUnit {
    #[serde(rename = "second")]
    Second,
    #[serde(rename = "minute")]
    Minute,
    #[serde(rename = "hour")]
    Hour,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RatelimitHeader {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
//TODO: use enum for model, but if there is a new model, we need to update the code
pub struct EmbeddingProviver {
    pub name: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
//TODO: use enum for model, but if there is a new model, we need to update the code
pub struct LlmProvider {
    pub name: String,
    pub provider: String,
    pub access_key: Option<String>,
    pub model: String,
    pub default: Option<bool>,
    pub stream: Option<bool>,
    pub rate_limits: Option<LlmRatelimit>,
}

impl Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub endpoint: Option<String>,
    // pub connect_timeout: Option<DurationString>,
    // pub timeout: Option<DurationString>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "type")]
    pub parameter_type: Option<String>,
    pub description: String,
    pub required: Option<bool>,
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointDetails {
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTarget {
    pub name: String,
    pub default: Option<bool>,
    pub description: String,
    pub endpoint: Option<EndpointDetails>,
    pub parameters: Option<Vec<Parameter>>,
    pub system_prompt: Option<String>,
    pub auto_llm_dispatch_on_response: Option<bool>,
}

#[cfg(test)]
mod test {
    use std::fs;

    use crate::configuration::GuardType;

    #[test]
    fn test_deserialize_configuration() {
        let ref_config =
            fs::read_to_string("../docs/source/resources/includes/curve_config_full_reference.yaml")
                .expect("reference config file not found");

        let config: super::Configuration = serde_yaml::from_str(&ref_config).unwrap();
        assert_eq!(config.version, "v0.1");

        let open_ai_provider = config
            .llm_providers
            .iter()
            .find(|p| p.name.to_lowercase() == "openai")
            .unwrap();
        assert_eq!(open_ai_provider.name.to_lowercase(), "openai");
        assert_eq!(
            open_ai_provider.access_key,
            Some("OPENAI_API_KEY".to_string())
        );
        assert_eq!(open_ai_provider.model, "gpt-4o");
        assert_eq!(open_ai_provider.default, Some(true));
        assert_eq!(open_ai_provider.stream, Some(true));

        let prompt_guards = config.prompt_guards.as_ref().unwrap();
        let input_guards = &prompt_guards.input_guards;
        let jailbreak_guard = input_guards.get(&GuardType::Jailbreak).unwrap();
        assert_eq!(
            jailbreak_guard
                .on_exception
                .as_ref()
                .unwrap()
                .forward_to_error_target,
            None
        );
        assert_eq!(
            jailbreak_guard.on_exception.as_ref().unwrap().error_handler,
            None
        );

        let prompt_targets = &config.prompt_targets;
        assert_eq!(prompt_targets.len(), 2);
        let prompt_target = prompt_targets
            .iter()
            .find(|p| p.name == "reboot_network_device")
            .unwrap();
        assert_eq!(prompt_target.name, "reboot_network_device");
        assert_eq!(prompt_target.default, None);

        let prompt_target = prompt_targets
            .iter()
            .find(|p| p.name == "information_extraction")
            .unwrap();
        assert_eq!(prompt_target.name, "information_extraction");
        assert_eq!(prompt_target.default, Some(true));
        assert_eq!(
            prompt_target.endpoint.as_ref().unwrap().name,
            "app_server".to_string()
        );
        assert_eq!(
            prompt_target.endpoint.as_ref().unwrap().path,
            Some("/agent/summary".to_string())
        );

        let error_target = config.error_target.as_ref().unwrap();
        assert_eq!(
            error_target.endpoint.as_ref().unwrap().name,
            "error_target_1".to_string()
        );
        assert_eq!(
            error_target.endpoint.as_ref().unwrap().path,
            Some("/error".to_string())
        );

        let tracing = config.tracing.as_ref().unwrap();
        assert_eq!(tracing.sampling_rate.unwrap(), 0.1);
    }
}

#[allow(warnings)]
mod bindings;

use amplifier_guest::{ChatResponse, ModelInfo, Provider, ProviderInfo, Usage};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
struct EchoProvider;

impl Provider for EchoProvider {
    fn name(&self) -> &str {
        "echo-provider"
    }

    fn get_info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "echo-provider".to_string(),
            display_name: "Echo Provider".to_string(),
            credential_env_vars: vec![],
            capabilities: vec!["chat".to_string()],
            defaults: HashMap::new(),
        }
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![ModelInfo {
            id: "echo-model".to_string(),
            display_name: "Echo Model".to_string(),
            context_window: 4096,
            max_output_tokens: 1024,
            capabilities: vec!["chat".to_string()],
            defaults: HashMap::new(),
            pricing: None,
        }])
    }

    fn complete(&self, _request: Value) -> Result<ChatResponse, String> {
        Ok(ChatResponse {
            content: vec![serde_json::json!({
                "type": "text",
                "text": "Echo response from WASM provider"
            })],
            tool_calls: None,
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                cost_usd: Some("0.000000000123456789".to_string()),
            }),
            finish_reason: Some("stop".to_string()),
            extra: HashMap::new(),
        })
    }

    fn parse_tool_calls(&self, _response: &ChatResponse) -> Vec<Value> {
        vec![]
    }
}

amplifier_guest::export_provider!(EchoProvider);

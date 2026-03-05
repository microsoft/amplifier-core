#[allow(warnings)]
mod bindings;

use amplifier_guest::{Tool, ToolSpec, ToolResult, Value};
use std::collections::HashMap;

#[derive(Default)]
struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo-tool"
    }

    fn get_spec(&self) -> ToolSpec {
        let mut params = HashMap::new();
        params.insert("type".to_string(), serde_json::json!("object"));
        params.insert(
            "properties".to_string(),
            serde_json::json!({"input": {"type": "string"}}),
        );
        ToolSpec {
            name: "echo-tool".to_string(),
            parameters: params,
            description: Some("Echoes input back as output".to_string()),
        }
    }

    fn execute(&self, input: Value) -> Result<ToolResult, String> {
        Ok(ToolResult {
            success: true,
            output: Some(input),
            error: None,
        })
    }
}

amplifier_guest::export_tool!(EchoTool);

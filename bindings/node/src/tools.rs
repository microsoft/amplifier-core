// ---------------------------------------------------------------------------
// JsToolBridge — lets TS authors implement Tool as plain TS objects
// ---------------------------------------------------------------------------

use napi::bindgen_prelude::Promise;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction};

/// Bridges a TypeScript tool object to Rust via `ThreadsafeFunction`.
///
/// In the hybrid coordinator pattern, these bridge objects are stored in a
/// JS-side Map (not in the Rust Coordinator). The JS orchestrator retrieves
/// them by name and calls `execute()` directly.
#[napi]
pub struct JsToolBridge {
    tool_name: String,
    tool_description: String,
    parameters_json: String,
    execute_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
}

#[napi]
impl JsToolBridge {
    #[napi(
        constructor,
        ts_args_type = "name: string, description: string, parametersJson: string, executeFn: (inputJson: string) => Promise<string>"
    )]
    pub fn new(
        name: String,
        description: String,
        parameters_json: String,
        execute_fn: JsFunction,
    ) -> Result<Self> {
        let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = execute_fn
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
                let input_str = ctx.env.create_string(&ctx.value)?;
                Ok(vec![input_str.into_unknown()])
            })?;

        Ok(Self {
            tool_name: name,
            tool_description: description,
            parameters_json,
            execute_fn: tsfn,
        })
    }

    #[napi(getter)]
    pub fn name(&self) -> &str {
        &self.tool_name
    }

    #[napi(getter)]
    pub fn description(&self) -> &str {
        &self.tool_description
    }

    #[napi]
    pub async fn execute(&self, input_json: String) -> Result<String> {
        let promise: Promise<String> = self
            .execute_fn
            .call_async(input_json)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
        promise.await.map_err(|e| Error::from_reason(e.to_string()))
    }

    #[napi]
    pub fn get_spec(&self) -> String {
        let params: serde_json::Value =
            serde_json::from_str(&self.parameters_json).unwrap_or_else(|e| {
                eprintln!(
                    "amplifier-core-node: JsToolBridge::get_spec() failed to parse parameters_json: {e}. Defaulting to empty object."
                );
                serde_json::Value::Object(serde_json::Map::new())
            });
        serde_json::json!({
            "name": self.tool_name,
            "description": self.tool_description,
            "parameters": params
        })
        .to_string()
    }
}

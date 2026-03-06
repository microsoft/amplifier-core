#[allow(warnings)]
mod bindings;

use amplifier_guest::Orchestrator;

/// Passthrough orchestrator that calls `echo-tool` via the kernel-service host import.
/// Proves that WASM guest modules can import and call host-provided functions.
#[derive(Default)]
struct PassthroughOrchestrator;

impl Orchestrator for PassthroughOrchestrator {
    fn execute(&self, prompt: String) -> Result<String, String> {
        // Build a JSON request for the echo-tool via the kernel service.
        let input = serde_json::json!({
            "name": "echo-tool",
            "input": { "prompt": prompt }
        });
        let request_bytes = serde_json::to_vec(&input).map_err(|e| e.to_string())?;

        // Call the kernel-service host import to execute the echo-tool.
        // This uses the WIT-generated import binding, not the placeholder in amplifier-guest.
        let result_bytes =
            bindings::amplifier::modules::kernel_service::execute_tool(&request_bytes)?;

        // Deserialize the result and return as a string.
        let result: serde_json::Value =
            serde_json::from_slice(&result_bytes).map_err(|e| e.to_string())?;
        Ok(result.to_string())
    }
}

amplifier_guest::export_orchestrator!(PassthroughOrchestrator);

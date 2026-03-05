#[allow(warnings)]
mod bindings;

use amplifier_guest::{HookAction, HookHandler, HookResult, Value};

#[derive(Default)]
struct DenyHook;

impl HookHandler for DenyHook {
    fn handle(&self, _event: &str, _data: Value) -> Result<HookResult, String> {
        Ok(HookResult {
            action: HookAction::Deny,
            reason: Some("Denied by WASM hook".to_string()),
            ..Default::default()
        })
    }
}

amplifier_guest::export_hook!(DenyHook);

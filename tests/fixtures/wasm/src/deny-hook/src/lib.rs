#[allow(warnings)]
mod bindings;

use amplifier_guest::{EventSubscription, HookAction, HookHandler, HookResult, Value};

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

    fn get_subscriptions(&self, _config: Value) -> Vec<EventSubscription> {
        vec![EventSubscription {
            event: "tool:pre".to_string(),
            priority: 0,
            name: "deny-all".to_string(),
        }]
    }
}

amplifier_guest::export_hook!(DenyHook);

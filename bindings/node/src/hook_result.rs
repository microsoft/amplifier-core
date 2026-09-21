// ---------------------------------------------------------------------------
// Structs — exported as TypeScript interfaces via #[napi(object)]
// ---------------------------------------------------------------------------

use amplifier_core::models::{ContextInjection, HookResult};

use crate::enums::{ApprovalDefault, ContextInjectionRole, HookAction, UserMessageLevel};

#[napi(object)]
pub struct JsContextInjection {
    pub content: String,
    pub role: ContextInjectionRole,
    pub ephemeral: bool,
    pub append_to_last_tool_result: bool,
    pub hook_name: String,
    pub event: String,
}

#[napi(object)]
pub struct JsHookResult {
    pub action: HookAction,
    pub reason: Option<String>,
    pub context_injection: Option<String>,
    pub context_injection_role: Option<ContextInjectionRole>,
    pub ephemeral: Option<bool>,
    pub suppress_output: Option<bool>,
    pub user_message: Option<String>,
    pub user_message_level: Option<UserMessageLevel>,
    pub user_message_source: Option<String>,
    pub approval_prompt: Option<String>,
    pub approval_timeout: Option<f64>,
    pub approval_default: Option<ApprovalDefault>,
    pub context_injections: Option<Vec<JsContextInjection>>,
}

// ---------------------------------------------------------------------------
// HookResult converter
// ---------------------------------------------------------------------------

pub(crate) fn hook_result_to_js(result: HookResult) -> JsHookResult {
    JsHookResult {
        action: result.action.into(),
        reason: result.reason,
        context_injection: result.context_injection,
        context_injection_role: Some(result.context_injection_role.into()),
        ephemeral: Some(result.ephemeral),
        suppress_output: Some(result.suppress_output),
        user_message: result.user_message,
        user_message_level: Some(result.user_message_level.into()),
        user_message_source: result.user_message_source,
        approval_prompt: result.approval_prompt,
        approval_timeout: Some(result.approval_timeout),
        approval_default: Some(result.approval_default.into()),
        context_injections: Some(
            result
                .context_injections
                .into_iter()
                .map(context_injection_to_js)
                .collect(),
        ),
    }
}

fn context_injection_to_js(injection: ContextInjection) -> JsContextInjection {
    JsContextInjection {
        content: injection.content,
        role: injection.role.into(),
        ephemeral: injection.ephemeral,
        append_to_last_tool_result: injection.append_to_last_tool_result,
        hook_name: injection.hook_name,
        event: injection.event,
    }
}

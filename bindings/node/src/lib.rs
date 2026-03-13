//! # amplifier-core Node.js bindings (Napi-RS)
//!
//! This module defines the FFI type contract between Rust and Node.js.
//! The enums and structs here are the authoritative boundary types — keep
//! the `From` impls in sync whenever upstream `amplifier_core::models` changes.
//!
//! Planned classes:
//!
//! | Rust struct       | JS class             |
//! |-------------------|----------------------|
//! | Session           | JsSession            |
//! | HookRegistry      | JsHookRegistry       |
//! | CancellationToken | JsCancellationToken  |
//! | Coordinator       | JsCoordinator        |

#[macro_use]
extern crate napi_derive;

pub mod cancellation;
pub mod coordinator;
pub mod enums;
pub mod errors;
pub mod hook_result;
pub mod hooks;
pub mod module_resolver;
pub mod session;
pub mod tools;

// Re-export public API so external consumers see a flat namespace
pub use cancellation::JsCancellationToken;
pub use coordinator::JsCoordinator;
pub use enums::{
    ApprovalDefault, ContextInjectionRole, HookAction, SessionState, UserMessageLevel,
};
pub use errors::{amplifier_error_to_js, JsAmplifierError};
pub use hook_result::JsHookResult;
pub use hooks::JsHookRegistry;
pub use module_resolver::{load_wasm_from_path, resolve_module, JsModuleManifest};
pub use session::JsAmplifierSession;
pub use tools::JsToolBridge;

#[napi]
pub fn hello() -> String {
    "Hello from amplifier-core native addon!".to_string()
}

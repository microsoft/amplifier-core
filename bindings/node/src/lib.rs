//! # amplifier-core Node.js bindings (Napi-RS)
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

#[napi]
pub fn hello() -> String {
    "Hello from amplifier-core native addon!".to_string()
}

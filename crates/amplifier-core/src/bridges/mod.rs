//! Transport bridge implementations.
//!
//! Each bridge wraps a remote module (gRPC, WASM, etc.) as an `Arc<dyn Trait>`,
//! making it indistinguishable from an in-process Rust module.

pub mod grpc_approval;
pub mod grpc_context;
pub mod grpc_hook;
pub mod grpc_orchestrator;
pub mod grpc_provider;
pub mod grpc_tool;
#[cfg(feature = "wasm")]
pub mod wasm_approval;
#[cfg(feature = "wasm")]
pub mod wasm_context;
#[cfg(feature = "wasm")]
pub mod wasm_hook;
#[cfg(feature = "wasm")]
pub mod wasm_tool;
#[cfg(feature = "wasm")]
pub mod wasm_provider;

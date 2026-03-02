//! Transport bridge implementations.
//!
//! Each bridge wraps a remote module (gRPC, WASM, etc.) as an `Arc<dyn Trait>`,
//! making it indistinguishable from an in-process Rust module.

pub mod grpc_provider;
pub mod grpc_tool;

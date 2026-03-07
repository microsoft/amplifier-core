//! gRPC bridge for remote orchestrator modules.
//!
//! [`GrpcOrchestratorBridge`] wraps an [`OrchestratorServiceClient`] (gRPC) and
//! implements the native [`Orchestrator`] trait, making a remote orchestrator
//! indistinguishable from a local one.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! use amplifier_core::bridges::grpc_orchestrator::GrpcOrchestratorBridge;
//! use amplifier_core::traits::Orchestrator;
//! use std::sync::Arc;
//!
//! let bridge = GrpcOrchestratorBridge::connect("http://localhost:50051", "session-abc").await?;
//! let orchestrator: Arc<dyn Orchestrator> = Arc::new(bridge);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use tonic::transport::Channel;

use crate::errors::{AmplifierError, SessionError};
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::orchestrator_service_client::OrchestratorServiceClient;
use crate::traits::{ContextManager, Orchestrator, Provider, Tool};

/// A bridge that wraps a remote gRPC `OrchestratorService` as a native [`Orchestrator`].
///
/// The client is held behind a [`tokio::sync::Mutex`] because
/// `OrchestratorServiceClient` methods take `&mut self` and we need to hold
/// the lock across `.await` points.
///
/// `session_id` is set at construction time and transmitted with every
/// `execute` call so the remote orchestrator can route KernelService
/// callbacks back to the correct session.
pub struct GrpcOrchestratorBridge {
    client: tokio::sync::Mutex<OrchestratorServiceClient<Channel>>,
    session_id: String,
}

impl GrpcOrchestratorBridge {
    /// Connect to a remote orchestrator service.
    ///
    /// # Arguments
    ///
    /// * `endpoint` — gRPC endpoint URL (e.g. `"http://localhost:50051"`).
    /// * `session_id` — Session identifier used for KernelService callback routing.
    pub async fn connect(
        endpoint: &str,
        session_id: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = OrchestratorServiceClient::connect(endpoint.to_string()).await?;

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
            session_id: session_id.to_string(),
        })
    }
}

impl Orchestrator for GrpcOrchestratorBridge {
    // Remote orchestrators access these subsystems via KernelService callbacks
    // using session_id routing. The parameters are intentionally not serialized
    // over gRPC.
    fn execute(
        &self,
        prompt: String,
        _context: Arc<dyn ContextManager>,
        _providers: HashMap<String, Arc<dyn Provider>>,
        _tools: HashMap<String, Arc<dyn Tool>>,
        _hooks: Value,
        _coordinator: Value,
    ) -> Pin<Box<dyn Future<Output = Result<String, AmplifierError>> + Send + '_>> {
        Box::pin(async move {
            log::debug!(
                "GrpcOrchestratorBridge::execute — context, providers, tools, hooks, and coordinator \
                 parameters are not transmitted via gRPC (remote orchestrator uses KernelService callbacks)"
            );
            let request = amplifier_module::OrchestratorExecuteRequest {
                prompt,
                session_id: self.session_id.clone(),
            };

            let response = {
                let mut client = self.client.lock().await;
                client.execute(request).await.map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!("gRPC: {}", e),
                    })
                })?
            };

            let resp = response.into_inner();

            if !resp.error.is_empty() {
                return Err(AmplifierError::Session(SessionError::Other {
                    message: resp.error,
                }));
            }

            Ok(resp.response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn assert_orchestrator_trait_object(_: Arc<dyn crate::traits::Orchestrator>) {}

    /// Compile-time check: GrpcOrchestratorBridge can be wrapped in Arc<dyn Orchestrator>.
    #[allow(dead_code)]
    fn grpc_orchestrator_bridge_is_orchestrator() {
        fn _check(bridge: GrpcOrchestratorBridge) {
            assert_orchestrator_trait_object(Arc::new(bridge));
        }
    }

    // ── S-4 regression: execute() discards 5 parameters ──────────────────────

    /// execute() discards 5 parameters; the structural gap must be documented
    /// with a clear doc comment and a log::debug!() call so the loss is
    /// visible at runtime.
    ///
    /// NOTE: we split at the `#[cfg(test)]` boundary so the test assertions
    /// themselves (which reference the searched tokens as string literals) do
    /// not produce false positives.
    #[test]
    fn execute_discarded_params_are_documented_and_logged() {
        let full_source = include_str!("grpc_orchestrator.rs");
        // Inspect only the implementation section (before the test module).
        let impl_source = full_source
            .split("\n#[cfg(test)]")
            .next()
            .expect("source must contain an impl section before #[cfg(test)]");

        assert!(
            impl_source.contains("log::debug!("),
            "execute() impl must contain a log::debug!() call for discarded parameters"
        );
        assert!(
            impl_source.contains("KernelService"),
            "execute() impl must reference KernelService in the explanation of discarded parameters"
        );
    }

    /// session_id must be stored in the struct and used in execute().
    ///
    /// This test verifies that the session_id placeholder (String::new()) has
    /// been replaced with an actual field that is set at construction time and
    /// threaded through the gRPC request for callback routing.
    ///
    /// NOTE: we split at the `#[cfg(test)]` boundary so the test assertions
    /// themselves (which reference the searched tokens as string literals) do
    /// not produce false positives.
    #[test]
    fn session_id_is_stored_and_used_in_execute() {
        let full_source = include_str!("grpc_orchestrator.rs");
        let impl_source = full_source
            .split("\n#[cfg(test)]")
            .next()
            .expect("source must contain an impl section before #[cfg(test)]");

        assert!(
            impl_source.contains("    session_id: String,"),
            "GrpcOrchestratorBridge struct must declare a `session_id: String` field"
        );
        assert!(
            impl_source.contains("self.session_id"),
            "execute() must use self.session_id (not a hardcoded placeholder)"
        );
        assert!(
            !impl_source.contains("session_id: String::new()"),
            "session_id: String::new() placeholder must be removed; use self.session_id instead"
        );
    }
}

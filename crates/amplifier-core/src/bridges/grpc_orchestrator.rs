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
//! let bridge = GrpcOrchestratorBridge::connect("http://localhost:50051").await?;
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
pub struct GrpcOrchestratorBridge {
    client: tokio::sync::Mutex<OrchestratorServiceClient<Channel>>,
}

impl GrpcOrchestratorBridge {
    /// Connect to a remote orchestrator service.
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = OrchestratorServiceClient::connect(endpoint.to_string()).await?;

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
        })
    }
}

impl Orchestrator for GrpcOrchestratorBridge {
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
            let request = amplifier_module::OrchestratorExecuteRequest {
                prompt,
                session_id: String::new(),
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
}

//! gRPC bridge for remote approval modules.
//!
//! [`GrpcApprovalBridge`] wraps an [`ApprovalServiceClient`] (gRPC) and
//! implements the native [`ApprovalProvider`] trait, making a remote approval
//! provider indistinguishable from a local one.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! use amplifier_core::bridges::grpc_approval::GrpcApprovalBridge;
//! use amplifier_core::traits::ApprovalProvider;
//! use std::sync::Arc;
//!
//! let bridge = GrpcApprovalBridge::connect("http://localhost:50051").await?;
//! let approval: Arc<dyn ApprovalProvider> = Arc::new(bridge);
//! # Ok(())
//! # }
//! ```

use std::future::Future;
use std::pin::Pin;

use tonic::transport::Channel;

use crate::errors::{AmplifierError, SessionError};
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::approval_service_client::ApprovalServiceClient;
use crate::models::{ApprovalRequest, ApprovalResponse};
use crate::traits::ApprovalProvider;

// TODO(grpc-v2): proto uses bare double for timeout, so None (no timeout) and
// Some(0.0) (expire immediately) are indistinguishable on the wire. Fix requires
// changing proto to optional double timeout.

/// Map an optional approval timeout to the wire value.
///
/// Because the proto field is a bare `double`, `None` (no timeout) is sent as
/// `0.0` — which is indistinguishable from "expire immediately". See the
/// `TODO(grpc-v2)` above.
fn map_approval_timeout(timeout: Option<f64>) -> f64 {
    timeout.unwrap_or_else(|| {
        log::debug!(
            "ApprovalRequest has no timeout — sending 0.0 on wire \
             (indistinguishable from 'expire immediately')"
        );
        0.0
    })
}

/// A bridge that wraps a remote gRPC `ApprovalService` as a native [`ApprovalProvider`].
///
/// The client is held behind a [`tokio::sync::Mutex`] because
/// `ApprovalServiceClient` methods take `&mut self` and we need to hold
/// the lock across `.await` points.
pub struct GrpcApprovalBridge {
    client: tokio::sync::Mutex<ApprovalServiceClient<Channel>>,
}

impl GrpcApprovalBridge {
    /// Connect to a remote approval service.
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = ApprovalServiceClient::connect(endpoint.to_string()).await?;

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
        })
    }
}

impl ApprovalProvider for GrpcApprovalBridge {
    fn request_approval(
        &self,
        request: ApprovalRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ApprovalResponse, AmplifierError>> + Send + '_>> {
        Box::pin(async move {
            let details_json = serde_json::to_string(&request.details).map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("gRPC: {}", e),
                })
            })?;

            let proto_request = amplifier_module::ApprovalRequest {
                tool_name: request.tool_name,
                action: request.action,
                details_json,
                risk_level: request.risk_level,
                timeout: map_approval_timeout(request.timeout),
            };

            let response = {
                let mut client = self.client.lock().await;
                client.request_approval(proto_request).await.map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!("gRPC: {}", e),
                    })
                })?
            };

            let proto_resp = response.into_inner();

            let reason = if proto_resp.reason.is_empty() {
                None
            } else {
                Some(proto_resp.reason)
            };

            Ok(ApprovalResponse {
                approved: proto_resp.approved,
                reason,
                remember: proto_resp.remember,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[allow(dead_code)]
    fn assert_approval_trait_object(_: Arc<dyn crate::traits::ApprovalProvider>) {}

    /// Compile-time check: GrpcApprovalBridge can be wrapped in Arc<dyn ApprovalProvider>.
    #[allow(dead_code)]
    fn grpc_approval_bridge_is_approval_provider() {
        fn _check(bridge: GrpcApprovalBridge) {
            assert_approval_trait_object(Arc::new(bridge));
        }
    }

    #[test]
    fn none_timeout_defaults_to_zero() {
        // When timeout is None, the wire value should be 0.0.
        let timeout: Option<f64> = None;
        let result = map_approval_timeout(timeout);
        assert!((result - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn some_timeout_is_preserved() {
        let timeout: Option<f64> = Some(30.0);
        let result = map_approval_timeout(timeout);
        assert!((result - 30.0).abs() < f64::EPSILON);
    }
}

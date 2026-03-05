#[allow(warnings)]
mod bindings;

use amplifier_guest::{ApprovalProvider, ApprovalRequest, ApprovalResponse};

#[derive(Default)]
struct AutoApprove;

impl ApprovalProvider for AutoApprove {
    fn request_approval(&self, _request: ApprovalRequest) -> Result<ApprovalResponse, String> {
        Ok(ApprovalResponse {
            approved: true,
            reason: Some("Auto-approved by WASM module".to_string()),
            remember: false,
        })
    }
}

amplifier_guest::export_approval!(AutoApprove);

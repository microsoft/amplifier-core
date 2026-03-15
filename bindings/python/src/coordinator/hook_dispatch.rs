//! Hook result dispatch for PyCoordinator.
//!
//! This module will contain `process_hook_result()` — the routing logic
//! that dispatches `HookResult` actions to the appropriate bridges:
//!
//! - `inject_context` → PyContextManagerBridge
//! - `ask_user` → PyApprovalProviderBridge
//! - `user_message` → PyDisplayServiceBridge
//! - `continue` / None → no-op
//!
//! Also: token budget tracking (injection_budget_per_turn, injection_size_limit).
//!
//! **Status:** Placeholder — implementation is Phase 3 work.

// Phase 3 will add:
// use super::PyCoordinator;
// use crate::bridges::{PyApprovalProviderBridge, PyContextManagerBridge, PyDisplayServiceBridge};
//
// #[pymethods]
// impl PyCoordinator {
//     fn process_hook_result(...) { ... }
// }

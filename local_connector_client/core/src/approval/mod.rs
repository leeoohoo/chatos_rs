// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod ai_agent;
mod fingerprint;
mod pending;
mod risk;
mod service;
mod types;
mod whitelist;

pub(crate) use ai_agent::{run_auto_approval_agent, AutoApprovalDecision};
pub(crate) use pending::{
    approve_pending_approval, deny_pending_approval, finish_in_progress_approval,
    list_in_progress_approvals, list_pending_approvals, start_in_progress_approval,
};
pub(crate) use service::{approval_project_key_from_request, CommandApprovalService};
pub(crate) use types::{
    ApprovalAiSettings, ApprovalDecision, ApprovalMemorySettings, ApprovalMode, ApprovalState,
    CommandApprovalRequest, ProjectApprovalState,
};

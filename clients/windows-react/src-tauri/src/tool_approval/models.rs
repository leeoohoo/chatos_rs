use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalMode {
    RequestApproval,
    AutoApproval,
    FullControl,
}

impl ToolApprovalMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::RequestApproval => "request_approval",
            Self::AutoApproval => "auto_approval",
            Self::FullControl => "full_control",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolApprovalDecision {
    Accept,
    AcceptForSession,
    Decline,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolApproval {
    pub invocation_id: String,
    pub invocation_version: u64,
    pub run_id: String,
    pub tool_name: String,
    pub command: String,
    pub purpose: Option<String>,
    pub cwd: String,
    pub source: String,
    pub risk: String,
    pub reason: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub available_decisions: Vec<ToolApprovalDecision>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalMutationInput {
    pub owner_user_id: String,
    pub invocation_id: String,
    pub expected_version: u64,
    pub decision: ToolApprovalDecision,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalResolution {
    pub invocation_id: String,
    pub invocation_version: u64,
    pub decision: ToolApprovalDecision,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolApprovalHistoryEntry {
    pub id: String,
    pub invocation_id: String,
    pub invocation_version: u64,
    pub command: String,
    pub purpose: Option<String>,
    pub cwd: String,
    pub source: String,
    pub mode: ToolApprovalMode,
    pub decision: ToolApprovalDecision,
    pub risk: String,
    pub reason: Option<String>,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalModelOption {
    pub id: String,
    pub display_name: String,
    pub model_name: String,
    pub provider: String,
    pub supports_reasoning: bool,
    pub thinking_levels: Vec<String>,
    pub default_thinking_level: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalSettings {
    pub default_mode: ToolApprovalMode,
    pub selected_model_id: Option<String>,
    pub selected_thinking_level: Option<String>,
    pub models: Vec<ToolApprovalModelOption>,
    pub automatic_reviewer_ready: bool,
    pub automatic_reviewer_detail: String,
    pub history: Vec<ToolApprovalHistoryEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolApprovalModeMutationInput {
    pub owner_user_id: String,
    pub mode: ToolApprovalMode,
    pub risk_acknowledged: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolApprovalModelMutationInput {
    pub owner_user_id: String,
    pub selected_model_id: Option<String>,
    pub selected_thinking_level: Option<String>,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalDecision {
    Accept,
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

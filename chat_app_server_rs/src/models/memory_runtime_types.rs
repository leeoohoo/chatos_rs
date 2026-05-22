use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeleteSummaryResultDto {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub reset_messages: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewRepairSummaryRunResultDto {
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
    pub pending_message_count: i64,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewRepairStatusDto {
    pub running: bool,
    pub running_job_count: i64,
    pub pending_message_count: i64,
    pub scope_session_count: usize,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub job_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReviewRepairSummaryRequestDto {
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotSelectedCommandDto {
    pub command_ref: Option<String>,
    pub name: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
    pub trigger: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotSystemMessageDto {
    pub id: String,
    pub source: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TurnRuntimeSnapshotContextItemDto {
    pub role: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub source: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotToolDto {
    pub name: String,
    pub server_name: String,
    pub server_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotUnavailableToolDto {
    pub server_name: String,
    pub tool_name: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TurnRuntimeSnapshotBuiltinMcpPromptDto {
    pub prompt_source_path: Option<String>,
    #[serde(default)]
    pub all_section_ids: Vec<String>,
    #[serde(default)]
    pub selected_section_ids: Vec<String>,
    #[serde(default)]
    pub omitted_section_ids: Vec<String>,
    #[serde(default)]
    pub requested_builtin_server_names: Vec<String>,
    #[serde(default)]
    pub active_builtin_server_names: Vec<String>,
    #[serde(default)]
    pub omitted_builtin_server_names: Vec<String>,
    pub runtime_limitations: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TurnRuntimeSnapshotRuntimeDto {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub contact_agent_id: Option<String>,
    pub remote_connection_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub workspace_root: Option<String>,
    pub mcp_enabled: Option<bool>,
    #[serde(default)]
    pub enabled_mcp_ids: Vec<String>,
    #[serde(default)]
    pub selected_commands: Vec<TurnRuntimeSnapshotSelectedCommandDto>,
    #[serde(default)]
    pub unavailable_builtin_tools: Vec<TurnRuntimeSnapshotUnavailableToolDto>,
    pub builtin_mcp_prompt: Option<TurnRuntimeSnapshotBuiltinMcpPromptDto>,
    pub actual_context_mode: Option<String>,
    #[serde(default)]
    pub actual_context_items: Vec<TurnRuntimeSnapshotContextItemDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncTurnRuntimeSnapshotRequestDto {
    pub user_message_id: Option<String>,
    pub status: Option<String>,
    pub snapshot_source: Option<String>,
    pub snapshot_version: Option<i64>,
    pub captured_at: Option<String>,
    pub system_messages: Option<Vec<TurnRuntimeSnapshotSystemMessageDto>>,
    pub tools: Option<Vec<TurnRuntimeSnapshotToolDto>>,
    pub runtime: Option<TurnRuntimeSnapshotRuntimeDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotDto {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub turn_id: String,
    pub user_message_id: Option<String>,
    pub status: String,
    pub snapshot_source: String,
    pub snapshot_version: i64,
    pub captured_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub system_messages: Vec<TurnRuntimeSnapshotSystemMessageDto>,
    #[serde(default)]
    pub tools: Vec<TurnRuntimeSnapshotToolDto>,
    pub runtime: Option<TurnRuntimeSnapshotRuntimeDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnRuntimeSnapshotLookupResponseDto {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub status: String,
    pub snapshot_source: String,
    pub snapshot: Option<TurnRuntimeSnapshotDto>,
}

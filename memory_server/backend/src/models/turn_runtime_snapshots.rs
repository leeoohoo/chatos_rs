use serde::{Deserialize, Serialize};

fn default_unknown_status() -> String {
    "unknown".to_string()
}

fn default_snapshot_source() -> String {
    "captured".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRuntimeSnapshotSystemMessage {
    pub id: String,
    pub source: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRuntimeSnapshotTool {
    pub name: String,
    pub server_name: String,
    pub server_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRuntimeSnapshotSelectedCommand {
    pub command_ref: Option<String>,
    pub name: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
    pub trigger: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnRuntimeSnapshotRuntime {
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
    pub selected_commands: Vec<TurnRuntimeSnapshotSelectedCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRuntimeSnapshot {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub turn_id: String,
    pub user_message_id: Option<String>,
    #[serde(default = "default_unknown_status")]
    pub status: String,
    #[serde(default = "default_snapshot_source")]
    pub snapshot_source: String,
    #[serde(default = "super::default_i64_1")]
    pub snapshot_version: i64,
    pub captured_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub system_messages: Vec<TurnRuntimeSnapshotSystemMessage>,
    #[serde(default)]
    pub tools: Vec<TurnRuntimeSnapshotTool>,
    pub runtime: Option<TurnRuntimeSnapshotRuntime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTurnRuntimeSnapshotRequest {
    pub user_message_id: Option<String>,
    pub status: Option<String>,
    pub snapshot_source: Option<String>,
    pub snapshot_version: Option<i64>,
    pub captured_at: Option<String>,
    pub system_messages: Option<Vec<TurnRuntimeSnapshotSystemMessage>>,
    pub tools: Option<Vec<TurnRuntimeSnapshotTool>>,
    pub runtime: Option<TurnRuntimeSnapshotRuntime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRuntimeSnapshotLookupResponse {
    pub session_id: String,
    pub turn_id: Option<String>,
    #[serde(default = "default_unknown_status")]
    pub status: String,
    pub snapshot_source: String,
    pub snapshot: Option<TurnRuntimeSnapshot>,
}

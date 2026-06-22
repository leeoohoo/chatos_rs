use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineThreadSnapshot {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub snapshot_type: String,
    pub user_message_id: Option<String>,
    pub status: String,
    pub snapshot_source: String,
    pub snapshot_version: i64,
    pub payload: Option<Value>,
    pub metadata: Option<Value>,
    pub captured_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertThreadSnapshotRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub user_message_id: Option<String>,
    pub status: Option<String>,
    pub snapshot_source: Option<String>,
    pub snapshot_version: Option<i64>,
    pub payload: Option<Value>,
    pub metadata: Option<Value>,
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSnapshotLookupResponse {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub snapshot_type: String,
    pub status: String,
    pub snapshot_source: String,
    pub snapshot: Option<EngineThreadSnapshot>,
}

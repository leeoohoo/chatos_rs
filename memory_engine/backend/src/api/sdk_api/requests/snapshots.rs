use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkGetLatestThreadSnapshotRequest {
    pub tenant_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SdkGetThreadSnapshotByTurnRequest {
    pub tenant_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SdkUpsertThreadSnapshotRequest {
    pub tenant_id: String,
    pub user_message_id: Option<String>,
    pub status: Option<String>,
    pub snapshot_source: Option<String>,
    pub snapshot_version: Option<i64>,
    pub payload: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub captured_at: Option<String>,
}

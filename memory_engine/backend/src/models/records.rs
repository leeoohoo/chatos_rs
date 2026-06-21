use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_pending;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRecord {
    pub id: String,
    pub thread_id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub external_record_id: Option<String>,
    pub role: String,
    pub record_type: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    #[serde(default = "default_pending")]
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertRecordInput {
    pub id: String,
    pub external_record_id: Option<String>,
    pub role: String,
    pub record_type: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSyncRecordsRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub records: Vec<UpsertRecordInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSyncRecordsResponse {
    pub thread_id: String,
    pub received_count: usize,
    pub upserted_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadRecordsPageResponse {
    pub items: Vec<EngineRecord>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecordSlice {
    pub turn_id: String,
    pub user_record: EngineRecord,
    pub final_assistant_record: Option<EngineRecord>,
    pub has_process: bool,
    pub tool_call_count: usize,
    pub thinking_count: usize,
    pub process_message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineCompactTurn {
    pub id: String,
    pub thread_id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub record_type: String,
    pub turn_id: String,
    pub user_record_id: String,
    pub user_created_at: String,
    pub user_record: EngineRecord,
    pub final_assistant_record: Option<EngineRecord>,
    pub has_process: bool,
    pub tool_call_count: usize,
    pub thinking_count: usize,
    pub process_message_count: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactTurnsResponse {
    pub items: Vec<TurnRecordSlice>,
    pub has_more: bool,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnProcessRecordsResponse {
    pub turn_id: String,
    pub items: Vec<EngineRecord>,
}

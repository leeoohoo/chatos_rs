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
pub struct SdkBatchSyncRecordsRequest {
    pub tenant_id: String,
    pub records: Vec<UpsertRecordInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSyncRecordsResponse {
    pub thread_id: String,
    pub received_count: usize,
    pub upserted_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkDeleteThreadRecordsRequest {
    pub tenant_id: String,
    pub record_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkCountThreadRecordsRequest {
    pub tenant_id: String,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkListThreadRecordsRequest {
    pub tenant_id: String,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub order: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkListCompactTurnsRequest {
    pub tenant_id: String,
    pub record_type: Option<String>,
    pub limit: Option<i64>,
    pub before_turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkGetTurnProcessRecordsRequest {
    pub tenant_id: String,
    pub record_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkGetRecordRequest {
    pub tenant_id: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountThreadRecordsResponse {
    pub count: i64,
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

#[cfg(test)]
mod tests {
    use super::EngineRecord;

    #[test]
    fn engine_record_defaults_summary_status_to_pending() {
        let record: EngineRecord = serde_json::from_value(serde_json::json!({
            "id": "rec-1",
            "thread_id": "thread-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "external_record_id": null,
            "role": "user",
            "record_type": "message",
            "content": "hello",
            "structured_payload": null,
            "metadata": null,
            "summary_id": null,
            "summarized_at": null,
            "created_at": "2026-05-21T00:00:00Z"
        }))
        .expect("record");

        assert_eq!(record.summary_status, "pending");
    }
}

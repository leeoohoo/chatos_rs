use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkListThreadRecordsRequest {
    pub tenant_id: String,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkListCompactTurnsRequest {
    pub tenant_id: String,
    pub record_type: Option<String>,
    pub limit: Option<i64>,
    pub before_turn_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkGetTurnProcessRecordsRequest {
    pub tenant_id: String,
    pub record_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkDeleteThreadRecordsRequest {
    pub tenant_id: String,
    pub record_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkCountThreadRecordsRequest {
    pub tenant_id: String,
    pub role: Option<String>,
    pub record_type: Option<String>,
    pub summary_status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkUpsertRecordInput {
    pub id: String,
    pub external_record_id: Option<String>,
    pub role: String,
    pub record_type: String,
    pub content: String,
    pub structured_payload: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SdkBatchSyncRecordsRequest {
    pub tenant_id: String,
    pub records: Vec<SdkUpsertRecordInput>,
}

#[derive(Debug, Deserialize)]
pub struct SdkGetRecordRequest {
    pub tenant_id: String,
    pub thread_id: Option<String>,
}

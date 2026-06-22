use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkGetThreadRequest {
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkUpsertThreadRequest {
    pub tenant_id: String,
    pub subject_id: String,
    pub thread_type: String,
    pub external_thread_id: Option<String>,
    pub title: Option<String>,
    pub labels: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkListThreadsRequest {
    pub tenant_id: String,
    pub subject_id: Option<String>,
    pub external_thread_id: Option<String>,
    pub session_id: Option<String>,
    pub contact_id: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub mapping_source: Option<String>,
    pub mapping_version: Option<String>,
    pub thread_label: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

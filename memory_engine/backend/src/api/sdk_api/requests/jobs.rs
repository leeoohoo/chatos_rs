use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkRunPendingSummariesRequest {
    pub tenant_id: Option<String>,
    pub max_threads: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SdkRunPendingRollupsRequest {
    pub tenant_id: Option<String>,
    pub summary_prompt: Option<String>,
    pub max_threads: Option<i64>,
    pub token_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub count_limit: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SdkRunSubjectMemoryScopesRequest {
    pub tenant_id: Option<String>,
    pub limit: Option<i64>,
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkComposeContextRequest {
    pub tenant_id: String,
    pub subject_id: Option<String>,
    pub related_subject_ids: Option<Vec<String>>,
    pub thread_id: String,
    pub policy: Option<crate::models::ComposeContextPolicy>,
}

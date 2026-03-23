use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct UserQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SystemContextRequest {
    pub(super) name: Option<String>,
    pub(super) content: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) is_active: Option<bool>,
    pub(super) app_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SystemContextAiGenerateRequest {
    pub(super) user_id: Option<String>,
    pub(super) scene: Option<String>,
    pub(super) style: Option<String>,
    pub(super) language: Option<String>,
    pub(super) output_format: Option<String>,
    pub(super) constraints: Option<Vec<String>>,
    pub(super) forbidden: Option<Vec<String>>,
    pub(super) candidate_count: Option<usize>,
    pub(super) ai_model_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SystemContextAiOptimizeRequest {
    pub(super) user_id: Option<String>,
    pub(super) content: Option<String>,
    pub(super) goal: Option<String>,
    pub(super) keep_intent: Option<bool>,
    pub(super) ai_model_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SystemContextAiEvaluateRequest {
    pub(super) content: Option<String>,
    pub(super) ai_model_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ActivateContextRequest {
    pub(super) user_id: Option<String>,
}

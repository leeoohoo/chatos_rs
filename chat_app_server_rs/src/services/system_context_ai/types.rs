use serde_json::Value;

pub struct GenerateDraftInput {
    pub user_id: Option<String>,
    pub scene: Option<String>,
    pub style: Option<String>,
    pub language: Option<String>,
    pub output_format: Option<String>,
    pub constraints: Option<Vec<String>>,
    pub forbidden: Option<Vec<String>>,
    pub candidate_count: Option<usize>,
    pub ai_model_config: Option<Value>,
}

pub struct OptimizeDraftInput {
    pub user_id: Option<String>,
    pub content: Option<String>,
    pub goal: Option<String>,
    pub keep_intent: Option<bool>,
    pub ai_model_config: Option<Value>,
}

pub struct EvaluateDraftInput {
    pub content: Option<String>,
    pub ai_model_config: Option<Value>,
}

#[derive(Debug)]
pub enum SystemContextAiError {
    BadRequest {
        message: String,
    },
    Upstream {
        message: String,
        raw: Option<String>,
    },
}

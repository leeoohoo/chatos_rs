use serde::{Deserialize, Serialize};

use super::{default_i64_0, default_i64_1};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelConfig {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_i64_0")]
    pub supports_images: i64,
    #[serde(default = "default_i64_0")]
    pub supports_reasoning: i64,
    #[serde(default = "default_i64_0")]
    pub supports_responses: i64,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertAiModelConfigRequest {
    pub user_id: String,
    pub name: String,
    pub provider: String,
    #[serde(alias = "model_name")]
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    pub enabled: Option<bool>,
}

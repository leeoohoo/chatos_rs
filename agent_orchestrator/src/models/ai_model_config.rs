use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelConfig {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub user_id: Option<String>,
    pub enabled: bool,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct AiModelConfigRow {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub user_id: Option<String>,
    pub enabled: i64,
    pub supports_images: i64,
    pub supports_reasoning: i64,
    pub supports_responses: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl AiModelConfigRow {
    pub fn to_model(self) -> AiModelConfig {
        let provider = if self.provider.trim().eq_ignore_ascii_case("openai") {
            "gpt".to_string()
        } else {
            self.provider.clone()
        };
        AiModelConfig {
            id: self.id,
            name: self.name,
            provider,
            model: self.model,
            thinking_level: self.thinking_level,
            api_key: self.api_key,
            base_url: self.base_url,
            user_id: self.user_id,
            enabled: self.enabled == 1,
            supports_images: self.supports_images == 1,
            supports_reasoning: self.supports_reasoning == 1,
            supports_responses: self.supports_responses == 1,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelConfig {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
    #[serde(default)]
    pub sync_warnings: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct AiModelConfigRow {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
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
            user_id: self.user_id,
            name: self.name,
            provider,
            model: self.model,
            thinking_level: self.thinking_level,
            task_usage_scenario: None,
            task_thinking_level: None,
            api_key: self.api_key,
            has_api_key: false,
            base_url: self.base_url,
            enabled: self.enabled == 1,
            supports_images: self.supports_images == 1,
            supports_reasoning: self.supports_reasoning == 1,
            supports_responses: self.supports_responses == 1,
            sync_warnings: Vec::new(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

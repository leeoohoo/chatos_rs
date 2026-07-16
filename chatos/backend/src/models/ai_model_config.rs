// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelConfig {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub prompt_vendor: Option<String>,
    pub model: String,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
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

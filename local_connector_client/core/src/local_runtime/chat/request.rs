// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct LocalChatModelOverrides {
    pub(crate) temperature: Option<f64>,
    pub(crate) thinking_level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LocalChatSendRequest {
    #[serde(alias = "session_id")]
    pub(crate) conversation_id: String,
    pub(crate) content: String,
    pub(crate) turn_id: Option<String>,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) model_config_id: Option<String>,
    pub(crate) reasoning_enabled: Option<bool>,
    pub(crate) system_prompt: Option<String>,
    #[serde(default)]
    pub(crate) attachments: Vec<Value>,
    #[serde(default)]
    pub(crate) ai_model_config: LocalChatModelOverrides,
}

pub(crate) fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

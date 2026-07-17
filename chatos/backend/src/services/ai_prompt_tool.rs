// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::services::llm_prompt_runner::run_text_prompt_with_model_config;

pub async fn run_text_prompt(
    model_config_id: Option<String>,
    user_id: Option<String>,
    ai_model_config: Option<Value>,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    default_model: &str,
    purpose: &str,
) -> Result<String, String> {
    run_text_prompt_with_model_config(
        model_config_id,
        user_id,
        ai_model_config.or_else(|| Some(json!({}))),
        system_prompt,
        user_prompt,
        max_tokens,
        default_model,
        purpose,
    )
    .await
}

pub fn parse_json_loose(raw: &str) -> Option<Value> {
    chatos_mcp_runtime::parse_json_loose(raw)
}

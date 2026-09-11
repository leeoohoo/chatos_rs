// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    routing::{get, post},
    Router,
};
use chatos_local_agent_protocol::{ContextStrategy, ModelProtocol};
use serde::Deserialize;

mod ai_model;

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct AiModelConfigRequest {
    id: Option<String>,
    name: Option<String>,
    provider: Option<String>,
    protocol: Option<ModelProtocol>,
    context_strategy: Option<ContextStrategy>,
    prompt_vendor: Option<String>,
    model: Option<String>,
    thinking_level: Option<String>,
    task_usage_scenario: Option<String>,
    task_thinking_level: Option<String>,
    temperature: Option<f64>,
    clear_temperature: Option<bool>,
    context_window_tokens: Option<i64>,
    clear_context_window_tokens: Option<bool>,
    max_output_tokens: Option<i64>,
    clear_max_output_tokens: Option<bool>,
    api_key: Option<String>,
    clear_api_key: Option<bool>,
    base_url: Option<String>,
    enabled: Option<bool>,
    task_enabled: Option<bool>,
    supports_images: Option<bool>,
    supports_reasoning: Option<bool>,
    supports_responses: Option<bool>,
    supports_streaming: Option<bool>,
    supports_native_compaction: Option<bool>,
    supports_input_token_count: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
struct AiModelSettingsRequest {
    user_id: Option<String>,
    model_request_max_retries: Option<i64>,
    memory_summary_model_config_id: Option<Option<String>>,
    memory_summary_thinking_level: Option<Option<String>>,
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/ai-model-configs",
            get(ai_model::list_ai_model_configs).post(ai_model::create_ai_model_config),
        )
        .route(
            "/api/ai-model-providers",
            get(ai_model::list_ai_model_providers).post(ai_model::create_ai_model_provider),
        )
        .route(
            "/api/ai-model-providers/{provider_id}",
            get(ai_model::get_ai_model_provider)
                .put(ai_model::update_ai_model_provider)
                .delete(ai_model::delete_ai_model_provider),
        )
        .route(
            "/api/ai-model-providers/{provider_id}/refresh",
            post(ai_model::refresh_ai_model_provider),
        )
        .route(
            "/api/ai-model-settings",
            get(ai_model::get_ai_model_settings).put(ai_model::put_ai_model_settings),
        )
        .route(
            "/api/ai-model-configs/{config_id}/models",
            get(ai_model::list_ai_provider_models),
        )
        .route(
            "/api/ai-model-configs/{config_id}",
            get(ai_model::get_ai_model_config)
                .put(ai_model::update_ai_model_config)
                .delete(ai_model::delete_ai_model_config),
        )
        .route(
            "/api/ai-model-configs/{config_id}/refresh",
            post(ai_model::refresh_ai_model_config),
        )
}

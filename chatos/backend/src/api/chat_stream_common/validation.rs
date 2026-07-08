// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use super::types::ChatStreamRequest;
use crate::services::shared_ai_runtime::resolve_shared_model_runtime_config_for_request;

pub(crate) async fn validate_chat_stream_request(
    req: &ChatStreamRequest,
    require_responses: bool,
) -> Result<(), (StatusCode, Json<Value>)> {
    let conversation_id = req.conversation_id.as_deref().unwrap_or_default().trim();
    let content = req.content.as_deref().unwrap_or_default();
    let has_text_content = !content.trim().is_empty();
    let has_attachments = req
        .attachments
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if conversation_id.is_empty() || (!has_text_content && !has_attachments) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": "conversation_id 不能为空，且 content 与 attachments 不能同时为空"}),
            ),
        ));
    }
    if require_responses {
        let model_runtime = resolve_shared_model_runtime_config_for_request(
            req.model_config_id.as_deref(),
            req.ai_model_config.as_ref(),
            req.conversation_id.as_deref(),
            req.user_id.as_deref(),
            "gpt-4o",
            req.reasoning_enabled,
            true,
        )
        .await
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "解析模型配置失败", "detail": err})),
            )
        })?;
        if !model_runtime.supports_responses {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "当前模型未启用 Responses API"})),
            ));
        }
    }
    Ok(())
}

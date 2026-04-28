use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use super::types::ChatStreamRequest;

pub(crate) fn validate_chat_stream_request(
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
    if require_responses
        && req
            .ai_model_config
            .as_ref()
            .and_then(|cfg| cfg.get("supports_responses").and_then(|v| v.as_bool()))
            != Some(true)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "当前模型未启用 Responses API"})),
        ));
    }
    Ok(())
}

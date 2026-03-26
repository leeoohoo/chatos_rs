use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::services::ai_common::{
    await_with_optional_abort, build_assistant_message_metadata, build_bearer_post_request,
    truncate_log,
};

use super::parser::{extract_output_text, extract_tool_calls};
use super::{should_persist_assistant_message, AiRequestHandler, AiResponse};

impl AiRequestHandler {
    pub(super) async fn handle_normal_request(
        &self,
        url: String,
        payload: serde_json::Value,
        session_id: Option<String>,
        turn_id: Option<String>,
        token: Option<CancellationToken>,
        force_identity_encoding: bool,
        persist_messages: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
    ) -> Result<AiResponse, String> {
        let req =
            build_bearer_post_request(&self.client, &url, &self.api_key, force_identity_encoding);
        let resp = await_with_optional_abort(req.json(&payload).send(), token).await?;

        let status = resp.status();
        let raw = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let err_text = truncate_log(&raw, 2000);
            error!(
                "[AI_V3] request failed: status={}, error={}",
                status, err_text
            );
            return Err(format!("status {}: {}", status, err_text));
        }

        let val: serde_json::Value = serde_json::from_str(raw.as_str()).map_err(|err| {
            format!(
                "invalid JSON response (status {}): {}; body_preview={}",
                status,
                err,
                truncate_log(raw.as_str(), 1200)
            )
        })?;

        let tool_calls = extract_tool_calls(&val);
        let content = extract_output_text(&val);
        let usage = val.get("usage").cloned();
        let finish_reason = val
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let provider_error = val.get("error").cloned().filter(|value| !value.is_null());
        let response_id = val
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        info!(
            "[AI_V3][prev-id] normal response parsed: session_id={}, turn_id={}, response_id={}, tool_call_count={}",
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            turn_id.clone().unwrap_or_else(|| "n/a".to_string()),
            response_id.as_deref().unwrap_or("none"),
            tool_calls
                .as_ref()
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0)
        );

        let should_persist = should_persist_assistant_message(
            content.as_str(),
            None,
            tool_calls.as_ref(),
            finish_reason.as_deref(),
        );
        if persist_messages && should_persist {
            if let Some(session_id) = session_id.clone() {
                let meta_val = build_assistant_message_metadata(
                    tool_calls.as_ref(),
                    response_id.as_deref(),
                    turn_id.as_deref(),
                    finish_reason.as_deref(),
                );
                let reasoning = None;
                if let Err(err) = self
                    .message_manager
                    .save_assistant_message(
                        &session_id,
                        &content,
                        None,
                        reasoning,
                        message_mode,
                        message_source,
                        meta_val,
                        tool_calls.clone(),
                    )
                    .await
                {
                    error!(
                        "[AI_V3] save assistant message failed: session_id={}, detail={}",
                        session_id, err
                    );
                }
            }
        } else if persist_messages {
            info!(
                "[AI_V3] skip assistant message persistence due to non-terminal empty response: session_id={}, turn_id={}, response_id={}, finish_reason={}",
                session_id.clone().unwrap_or_else(|| "n/a".to_string()),
                turn_id.clone().unwrap_or_else(|| "n/a".to_string()),
                response_id.as_deref().unwrap_or("none"),
                finish_reason.as_deref().unwrap_or("none")
            );
        }

        Ok(AiResponse {
            content,
            reasoning: None,
            tool_calls,
            finish_reason,
            provider_error,
            usage,
            response_id,
        })
    }
}

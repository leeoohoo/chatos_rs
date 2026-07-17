// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::chat::LocalRuntimeGuidance;
use crate::local_runtime::storage::AppendLocalMessageInput;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;
use super::messages::{message_response, LocalMessageResponse};

#[derive(Debug, Deserialize)]
pub(super) struct LocalGuidanceRequest {
    conversation_id: String,
    turn_id: String,
    content: String,
    #[serde(default)]
    attachments: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct LocalGuidanceResponse {
    accepted: bool,
    conversation_id: String,
    turn_id: String,
    guidance: LocalRuntimeGuidance,
    message: LocalMessageResponse,
}

pub(super) async fn send_guidance(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<LocalGuidanceRequest>,
) -> Result<Json<LocalGuidanceResponse>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let conversation_id = required(request.conversation_id, "conversation_id")?;
    let turn_id = required(request.turn_id, "turn_id")?;
    let content = required(request.content, "content")?;
    if !request.attachments.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_guidance_attachments_not_supported",
            "Local runtime guidance attachments are not available yet",
        ));
    }

    let guidance_id = format!("gd_{}", Uuid::new_v4().simple());
    let message_id = format!("lc_message_{}", Uuid::new_v4());
    let created_at = local_now_rfc3339();
    let guidance = LocalRuntimeGuidance {
        guidance_id: guidance_id.clone(),
        session_id: conversation_id.clone(),
        turn_id: turn_id.clone(),
        message_id: message_id.clone(),
        content: content.clone(),
        status: "queued".to_string(),
        created_at: created_at.clone(),
    };
    runtime
        .turn_control
        .enqueue_guidance(guidance.clone())
        .map_err(|error| LocalRuntimeApiError::conflict("local_runtime_turn_not_running", error))?;

    let metadata = json!({
        "conversation_turn_id": turn_id,
        "message_mode": "runtime_guidance",
        "message_source": "runtime_guidance",
        "runtime_origin": "local_device",
        "runtime_guidance": {
            "guidance_id": guidance_id,
            "status": "queued",
            "created_at": created_at,
        }
    });
    let saved = runtime
        .local_database()?
        .append_turn_message(AppendLocalMessageInput {
            session_id: conversation_id.clone(),
            owner_user_id: owner.owner_user_id,
            turn_id: turn_id.clone(),
            message_id: Some(message_id),
            role: "user".to_string(),
            content,
            reasoning: None,
            tool_calls_json: None,
            tool_call_id: None,
            metadata_json: Some(metadata.to_string()),
            created_at: Some(created_at),
        })
        .await;
    let saved = match saved {
        Ok(saved) => saved,
        Err(error) => {
            runtime.turn_control.remove_guidance(
                conversation_id.as_str(),
                turn_id.as_str(),
                guidance.guidance_id.as_str(),
            );
            return Err(LocalRuntimeApiError::internal(error.to_string()));
        }
    };

    Ok(Json(LocalGuidanceResponse {
        accepted: true,
        conversation_id,
        turn_id,
        guidance,
        message: message_response(saved),
    }))
}

fn required(value: String, field: &'static str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            format!("{field} is required"),
        ));
    }
    Ok(value)
}

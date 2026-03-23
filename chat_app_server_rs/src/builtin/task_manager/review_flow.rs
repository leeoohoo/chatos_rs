use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::mcp_tools::ToolStreamChunkCallback;
use crate::core::tool_io::text_result;
use crate::services::task_manager::{
    create_task_review, create_tasks_for_turn, wait_for_task_review_decision,
    TaskCreateReviewPayload, TaskReviewAction, REVIEW_TIMEOUT_ERR,
};
use crate::utils::events::Events;

use super::parsing::parse_task_drafts;
use super::ToolContext;

pub(super) fn handle_add_task(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let draft_tasks = parse_task_drafts(&args)?;
    if draft_tasks.is_empty() {
        return Err("tasks is required".to_string());
    }

    let timeout_ms = default_timeout_ms;

    let (review_payload, receiver) = block_on_result(create_task_review(
        ctx.session_id,
        ctx.conversation_turn_id,
        draft_tasks,
        timeout_ms,
    ))?;

    emit_review_required_event(ctx.on_stream_chunk.as_ref(), &review_payload);

    let decision = match block_on_result(wait_for_task_review_decision(
        review_payload.review_id.as_str(),
        receiver,
        review_payload.timeout_ms,
    )) {
        Ok(value) => value,
        Err(err) if err == REVIEW_TIMEOUT_ERR => {
            return Ok(cancelled_result("review_timeout"));
        }
        Err(err) => return Err(err),
    };

    match decision.action {
        TaskReviewAction::Confirm => {
            let tasks = block_on_result(create_tasks_for_turn(
                ctx.session_id,
                ctx.conversation_turn_id,
                decision.tasks,
            ))?;
            Ok(text_result(json!({
                "confirmed": true,
                "cancelled": false,
                "created_count": tasks.len(),
                "tasks": tasks,
                "session_id": ctx.session_id,
                "conversation_turn_id": ctx.conversation_turn_id,
            })))
        }
        TaskReviewAction::Cancel => {
            let reason = decision
                .reason
                .unwrap_or_else(|| "user_cancelled".to_string());
            Ok(cancelled_result(reason.as_str()))
        }
    }
}

fn emit_review_required_event(
    on_stream_chunk: Option<&ToolStreamChunkCallback>,
    payload: &TaskCreateReviewPayload,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };

    let event_payload = json!({
        "event": Events::TASK_CREATE_REVIEW_REQUIRED,
        "data": payload,
    });

    if let Ok(serialized) = serde_json::to_string(&event_payload) {
        callback(serialized);
    }
}

fn cancelled_result(reason: &str) -> Value {
    text_result(json!({
        "confirmed": false,
        "cancelled": true,
        "reason": reason,
    }))
}

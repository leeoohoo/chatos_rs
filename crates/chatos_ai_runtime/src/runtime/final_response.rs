use tracing::{info, warn};

use crate::request::AiResponse;

use super::options::AiRuntimeOptions;
use super::report::AiRuntimeResult;
use super::EMPTY_FINAL_RESPONSE_ERROR;

pub(super) enum FinalResponseAction {
    AskForFollowup,
    Complete,
}

pub(super) fn handle_response_without_tool_calls(
    response: &AiResponse,
    options: &AiRuntimeOptions,
    iteration: usize,
    max_iterations: usize,
    followup_attempted: bool,
) -> Result<FinalResponseAction, String> {
    if response.content.trim().is_empty() {
        if !followup_attempted && iteration < max_iterations {
            warn!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                response_id = response.response_id.as_deref().unwrap_or(""),
                finish_reason = response.finish_reason.as_deref().unwrap_or(""),
                "ai runtime received empty final response; asking model for final result"
            );
            return Ok(FinalResponseAction::AskForFollowup);
        }
        warn!(
            conversation_id = options.conversation_id.as_deref().unwrap_or(""),
            conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
            iteration,
            response_id = response.response_id.as_deref().unwrap_or(""),
            finish_reason = response.finish_reason.as_deref().unwrap_or(""),
            "ai runtime failed after empty final response"
        );
        return Err(EMPTY_FINAL_RESPONSE_ERROR.to_string());
    }

    info!(
        conversation_id = options.conversation_id.as_deref().unwrap_or(""),
        conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
        iteration,
        response_id = response.response_id.as_deref().unwrap_or(""),
        finish_reason = response.finish_reason.as_deref().unwrap_or(""),
        content_chars = response.content.chars().count(),
        "ai runtime completed without tool calls"
    );
    Ok(FinalResponseAction::Complete)
}

pub(super) fn runtime_result_from_response(response: AiResponse) -> AiRuntimeResult {
    AiRuntimeResult {
        content: response.content,
        reasoning: response.reasoning,
        tool_calls: response.tool_calls,
        finish_reason: response.finish_reason,
        usage: response.usage,
        response_id: response.response_id,
    }
}

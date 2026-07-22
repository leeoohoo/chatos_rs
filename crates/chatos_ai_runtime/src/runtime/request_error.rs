// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use tracing::{info, warn};

use crate::error_policy::{
    handle_transient_retry, is_context_length_exceeded_error, is_missing_tool_call_error,
    is_request_body_too_large_error,
};
use crate::traits::ModelRequest;
use crate::DEFAULT_MODEL_REQUEST_MAX_RETRIES;

use super::input_items::merge_pending_tool_turn_into_input;
use super::options::AiRuntimeOptions;

pub(super) enum ModelRequestErrorAction {
    ReplayMissingToolTurn(Value),
    ContextRecovered,
    RetryRequest,
    Fail(String),
}

pub(super) async fn handle_model_request_error(
    err: String,
    request: &ModelRequest,
    options: &AiRuntimeOptions,
    iteration: usize,
    missing_tool_turn_replay_attempted: bool,
    pending_tool_calls: Option<&[Value]>,
    pending_tool_outputs: Option<&[Value]>,
    context_overflow_recovery_attempted: &mut bool,
    transient_retry_count: &mut usize,
) -> Result<ModelRequestErrorAction, String> {
    if !missing_tool_turn_replay_attempted
        && request.supports_responses
        && is_missing_tool_call_error(err.as_str())
    {
        let repaired_input = merge_pending_tool_turn_into_input(
            request.input.clone(),
            pending_tool_calls,
            pending_tool_outputs,
        );
        if repaired_input != request.input {
            warn!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                error = err.as_str(),
                "ai runtime replaying pending tool turn after provider rejected incomplete tool exchange"
            );
            return Ok(ModelRequestErrorAction::ReplayMissingToolTurn(
                repaired_input,
            ));
        }
    }

    let should_try_context_recovery = !*context_overflow_recovery_attempted
        && (is_context_length_exceeded_error(err.as_str())
            || is_request_body_too_large_error(err.as_str()));
    if should_try_context_recovery {
        if let Some(refresh) = &options.iterative_context_refresh {
            info!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                error = err.as_str(),
                "ai runtime attempting context overflow recovery"
            );
            *context_overflow_recovery_attempted = true;
            match refresh
                .try_recover_from_context_overflow(&options.callbacks)
                .await
            {
                Ok(true) => {
                    return Ok(ModelRequestErrorAction::ContextRecovered);
                }
                Ok(false) => {}
                Err(recovery_err) => {
                    warn!("memory active summary recovery failed: {}", recovery_err);
                }
            }
        }
    }

    if handle_transient_retry(
        "ai runtime model request",
        err.as_str(),
        transient_retry_count,
        request
            .max_transient_retries
            .unwrap_or(DEFAULT_MODEL_REQUEST_MAX_RETRIES),
    )
    .await?
    {
        return Ok(ModelRequestErrorAction::RetryRequest);
    }

    Ok(ModelRequestErrorAction::Fail(err))
}

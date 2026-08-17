// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::error_policy::{
    classify_transient_retry, is_response_parse_error, is_upstream_connection_interrupted_error,
    should_retry_without_stream, TransientRetryAction,
};
use crate::model_config::{effective_responses_support, supports_previous_response_id};
use crate::tool_call::tool_calls_value_has_items;
use crate::traits::{ModelRequest, DEFAULT_MODEL_REQUEST_MAX_RETRIES};
use crate::{RuntimeFinalResponseAction, RuntimeFinalResponseContext};

use super::input_items::{append_runtime_input_items, input_item_count, json_value_size_bytes};
use super::model_request::dispatch_model_request;
use super::{
    count_iteration_input_tokens, downgraded_thinking_level, estimated_iteration_input_tokens,
    prepare_iteration_request, runtime_result_from_response, AiRuntime, AiRuntimeOptions,
    AiRuntimeResult, ACTIVE_CONTEXT_COMPACTION_INPUT_TOKENS, MAX_ACTIVE_CONTEXT_COMPACTION_PASSES,
};

#[derive(Clone)]
pub struct AiSingleStepRequest {
    pub model_request: ModelRequest,
    pub runtime_options: AiRuntimeOptions,
    pub iteration: usize,
    pub reason: String,
    pub model_attempt: usize,
    pub force_non_stream: bool,
    pub force_identity_encoding: bool,
}

impl AiSingleStepRequest {
    pub fn new(model_request: ModelRequest, runtime_options: AiRuntimeOptions) -> Self {
        Self {
            model_request,
            runtime_options,
            iteration: 1,
            reason: "initial".to_string(),
            model_attempt: 1,
            force_non_stream: false,
            force_identity_encoding: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.iteration == 0 {
            return Err("single model step iteration must be greater than zero".to_string());
        }
        if self.model_attempt == 0 {
            return Err("single model step attempt must be greater than zero".to_string());
        }
        if self.reason.trim().is_empty() {
            return Err("single model step reason must not be empty".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum AiSingleStepOutcome {
    Final(AiRuntimeResult),
    ToolCommand {
        response: AiRuntimeResult,
        tool_calls: Value,
    },
    Continue {
        response: AiRuntimeResult,
        input_items: Vec<Value>,
        reason: String,
    },
    Retry {
        error: String,
        retry_kind: String,
        next_model_attempt: usize,
        backoff_ms: u64,
        disable_stream: bool,
        downgrade_thinking_to: Option<String>,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

pub(super) async fn execute_once(
    runtime: &AiRuntime,
    request: AiSingleStepRequest,
) -> Result<AiSingleStepOutcome, String> {
    request.validate()?;
    if request.runtime_options.is_aborted() {
        return Ok(AiSingleStepOutcome::Cancelled);
    }
    if request.iteration > runtime.max_iterations {
        return Ok(AiSingleStepOutcome::Failed {
            error: "达到最大迭代次数".to_string(),
        });
    }

    let AiSingleStepRequest {
        mut model_request,
        runtime_options,
        iteration,
        reason,
        model_attempt,
        force_non_stream,
        force_identity_encoding,
    } = request;
    model_request.supports_responses = effective_responses_support(
        model_request.provider.as_str(),
        model_request.base_url.as_str(),
        model_request.supports_responses,
    );
    if !supports_previous_response_id(
        model_request.provider.as_str(),
        model_request.base_url.as_str(),
    ) {
        model_request.previous_response_id = None;
    }
    if let Some(refresh) = &runtime_options.iterative_context_refresh {
        match refresh
            .wait_for_inflight_summary(&runtime_options.callbacks)
            .await
        {
            Ok(true) => {
                model_request.previous_response_id = None;
                model_request.input = refresh.compose_input().await?;
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(
                conversation_id = runtime_options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = runtime_options
                    .conversation_turn_id
                    .as_deref()
                    .unwrap_or(""),
                iteration,
                error = error.as_str(),
                "ai runtime could not observe in-flight context summary"
            ),
        }
    }
    if let Some(executor) = &runtime.tool_executor {
        let tools = executor.available_tools();
        if !tools.is_empty() {
            model_request.tools = tools;
        }
    }
    let (mut iteration_request, lifecycle_before) =
        prepare_iteration_request(&model_request, &runtime_options, iteration, reason.as_str())
            .await?;
    if let Some(refresh) = runtime_options
        .iterative_context_refresh
        .as_ref()
        .filter(|refresh| refresh.has_memory_composer())
    {
        let mut remaining_input_tokens = None;
        let mut count_source = "local_estimate";
        for compaction_pass in 0..=MAX_ACTIVE_CONTEXT_COMPACTION_PASSES {
            let count = count_iteration_input_tokens(
                &runtime.request_handler,
                &iteration_request,
                &runtime_options,
            )
            .await;
            remaining_input_tokens = Some(count.tokens);
            count_source = count.source;
            tracing::info!(
                conversation_id = runtime_options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = runtime_options
                    .conversation_turn_id
                    .as_deref()
                    .unwrap_or(""),
                iteration,
                input_tokens = count.tokens,
                token_count_source = count.source,
                compaction_threshold = ACTIVE_CONTEXT_COMPACTION_INPUT_TOKENS,
                "ai runtime measured model input context"
            );
            if count.tokens <= ACTIVE_CONTEXT_COMPACTION_INPUT_TOKENS {
                break;
            }
            if compaction_pass == MAX_ACTIVE_CONTEXT_COMPACTION_PASSES {
                break;
            }
            match refresh
                .compact_active_context(&runtime_options.callbacks)
                .await
            {
                Ok(true) => {
                    model_request.previous_response_id = None;
                    model_request.input = refresh.compose_input().await?;
                    iteration_request.input = append_runtime_input_items(
                        model_request.input.clone(),
                        lifecycle_before.input_items.as_slice(),
                    );
                    iteration_request.previous_response_id = None;
                }
                Ok(false) => break,
                Err(error) => {
                    tracing::warn!(
                        conversation_id = runtime_options.conversation_id.as_deref().unwrap_or(""),
                        conversation_turn_id = runtime_options
                            .conversation_turn_id
                            .as_deref()
                            .unwrap_or(""),
                        iteration,
                        error = error.as_str(),
                        "ai runtime proactive context compaction failed"
                    );
                    break;
                }
            }
        }
        let remaining_input_tokens = remaining_input_tokens
            .unwrap_or_else(|| estimated_iteration_input_tokens(&iteration_request));
        if remaining_input_tokens > ACTIVE_CONTEXT_COMPACTION_INPUT_TOKENS {
            return Ok(AiSingleStepOutcome::Failed {
                error: format!(
                    "主动上下文压缩后输入仍为 {} tokens（计数来源：{}），已停止本次模型请求以避免异常消耗",
                    remaining_input_tokens, count_source
                ),
            });
        }
    }
    let request_input_items = crate::turn::input_value_to_items(iteration_request.input.clone());
    let response = dispatch_model_request(
        &runtime.request_handler,
        &iteration_request,
        &runtime_options,
        iteration,
        reason.as_str(),
        input_item_count(&iteration_request.input),
        json_value_size_bytes(&iteration_request.input),
        iteration_request.tools.len(),
        model_attempt,
        lifecycle_before.stream_output,
        !force_non_stream,
        force_identity_encoding,
    )
    .await;
    let mut response = match response {
        Ok(response) => response,
        Err(error) => {
            return Ok(retry_or_fail(
                error,
                &iteration_request,
                model_attempt,
                !force_non_stream,
            ))
        }
    };
    if runtime_options.is_aborted() {
        return Ok(AiSingleStepOutcome::Cancelled);
    }

    if let Some(tool_calls) = response
        .tool_calls
        .clone()
        .filter(|value| tool_calls_value_has_items(Some(value)))
    {
        runtime
            .save_assistant_record(
                &runtime_options,
                &response,
                Some(tool_calls.clone()),
                Some("tool_calls".to_string()),
                None,
            )
            .await?;
        let mut response = runtime_result_from_response(response);
        response.request_input_items = request_input_items;
        return Ok(AiSingleStepOutcome::ToolCommand {
            response,
            tool_calls,
        });
    }

    if response.content.trim().is_empty() {
        let mut response = runtime_result_from_response(response);
        response.request_input_items = request_input_items;
        return Ok(AiSingleStepOutcome::Continue {
            response,
            input_items: vec![super::input_items::empty_final_response_followup_item()],
            reason: "empty_final_response_followup".to_string(),
        });
    }

    if let Some(hook) = &runtime_options.lifecycle_hook {
        match hook
            .after_final_response(RuntimeFinalResponseContext {
                conversation_id: runtime_options.conversation_id.clone(),
                conversation_turn_id: runtime_options.conversation_turn_id.clone(),
                iteration,
                reason: reason.clone(),
                response: response.clone(),
            })
            .await?
        {
            RuntimeFinalResponseAction::Accept => {}
            RuntimeFinalResponseAction::Replace(replacement) => response = *replacement,
            RuntimeFinalResponseAction::Continue {
                input_items,
                reason,
            } => {
                let mut response = runtime_result_from_response(response);
                response.request_input_items = request_input_items;
                return Ok(AiSingleStepOutcome::Continue {
                    response,
                    input_items,
                    reason: normalized_reason(reason, "lifecycle_followup"),
                });
            }
        }
    }
    let lifecycle_metadata = if let Some(hook) = &runtime_options.lifecycle_hook {
        hook.final_response_metadata(RuntimeFinalResponseContext {
            conversation_id: runtime_options.conversation_id.clone(),
            conversation_turn_id: runtime_options.conversation_turn_id.clone(),
            iteration,
            reason,
            response: response.clone(),
        })
        .await?
    } else {
        None
    };
    runtime
        .save_assistant_record(
            &runtime_options,
            &response,
            response.tool_calls.clone(),
            None,
            lifecycle_metadata,
        )
        .await?;
    let mut result = runtime_result_from_response(response);
    result.request_input_items = request_input_items;
    Ok(AiSingleStepOutcome::Final(result))
}

fn retry_or_fail(
    error: String,
    request: &ModelRequest,
    model_attempt: usize,
    provider_stream: bool,
) -> AiSingleStepOutcome {
    let completed_retries = model_attempt.saturating_sub(1);
    let max_retries = request
        .max_transient_retries
        .unwrap_or(DEFAULT_MODEL_REQUEST_MAX_RETRIES);
    match classify_transient_retry(error.as_str(), completed_retries, max_retries) {
        Some(TransientRetryAction::Retry {
            retry_kind,
            next_retry_count,
            backoff_ms,
        }) => AiSingleStepOutcome::Retry {
            error: error.clone(),
            retry_kind: retry_kind.to_string(),
            next_model_attempt: next_retry_count.saturating_add(1),
            backoff_ms,
            disable_stream: provider_stream && should_retry_without_stream(error.as_str()),
            downgrade_thinking_to: if is_response_parse_error(error.as_str())
                || is_upstream_connection_interrupted_error(error.as_str())
            {
                downgraded_thinking_level(request.thinking_level.as_deref())
            } else {
                None
            },
        },
        Some(TransientRetryAction::Exhausted { error_message }) => AiSingleStepOutcome::Failed {
            error: error_message,
        },
        None => AiSingleStepOutcome::Failed { error },
    }
}

fn normalized_reason(reason: String, fallback: &str) -> String {
    let reason = reason.trim();
    if reason.is_empty() {
        fallback.to_string()
    } else {
        reason.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_is_returned_as_data_instead_of_sleeping_or_looping() {
        let request = ModelRequest::openai_compatible(
            "http://127.0.0.1:1",
            "test",
            "test",
            "openai",
            Value::Array(Vec::new()),
        )
        .with_max_transient_retries(Some(3));
        let outcome = retry_or_fail(
            "connection reset before message completed".to_string(),
            &request,
            1,
            true,
        );
        match outcome {
            AiSingleStepOutcome::Retry {
                next_model_attempt,
                backoff_ms,
                ..
            } => {
                assert_eq!(next_model_attempt, 2);
                assert!(backoff_ms > 0);
            }
            other => panic!("expected retry continuation, got {other:?}"),
        }
    }

    #[test]
    fn request_identity_rejects_zero_attempts() {
        let mut request = AiSingleStepRequest::new(
            ModelRequest::openai_compatible(
                "http://127.0.0.1:1",
                "test",
                "test",
                "openai",
                Value::Array(Vec::new()),
            ),
            AiRuntimeOptions::for_conversation("conversation-1"),
        );
        request.model_attempt = 0;
        assert_eq!(
            request.validate().unwrap_err(),
            "single model step attempt must be greater than zero"
        );
    }
}

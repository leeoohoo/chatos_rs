mod request_support;
mod stream_support;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use self::request_support::await_with_optional_abort;
pub(crate) use self::request_support::{
    attach_ai_client_success_extra, build_abort_token, build_ai_client_success_payload,
    build_assistant_message_metadata, build_user_content_parts, classify_user_facing_ai_error,
    completion_failed_error, handle_transient_retry, is_non_terminal_response_status,
    is_retryable_provider_backpressure_error, is_task_runner_async_plan_message_mode,
    normalize_task_runner_async_plan_metadata, normalize_task_runner_async_tool_call_metadata,
    normalize_turn_id, persist_assistant_response_with_policy,
    persist_user_message_and_build_content_parts, should_persist_assistant_message,
    terminal_empty_response_error, AssistantResponsePersistenceRequest,
    TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
#[cfg(test)]
pub(crate) use self::stream_support::drain_sse_json_events;
#[cfg(test)]
pub(crate) use self::stream_support::{
    aborted_tool_results_if_needed, build_aborted_tool_results, build_tool_stream_callback,
    build_tools_end_payload, consume_sse_stream, emit_stream_callbacks,
    parsed_stream_response_is_empty,
};
pub(crate) use self::stream_support::{
    build_tool_result_metadata, execute_tool_lifecycle, AiStreamCallbacks,
};

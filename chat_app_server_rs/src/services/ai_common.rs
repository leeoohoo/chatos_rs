mod request_support;
mod stream_support;
#[cfg(test)]
mod tests;

pub(crate) use self::request_support::{
    attach_ai_client_success_extra, build_abort_token, build_ai_client_success_payload,
    build_assistant_message_metadata, build_user_content_parts, build_user_message_metadata,
    classify_user_facing_ai_error, completion_failed_error, handle_transient_retry,
    is_non_terminal_response_status, is_retryable_provider_backpressure_error,
    normalize_reasoning_effort, normalize_turn_id, persist_assistant_response_with_policy,
    persist_user_message_and_build_content_parts, read_error_response_text,
    send_bearer_json_request, should_persist_assistant_message, terminal_empty_response_error,
    validate_request_payload_size, AssistantResponsePersistenceRequest,
};
#[cfg(test)]
pub(crate) use self::request_support::{
    await_with_optional_abort, is_response_parse_error, is_transient_network_error,
    is_transient_transport_or_parse_error,
};
#[cfg(test)]
pub(crate) use self::stream_support::drain_sse_json_events;
#[cfg(test)]
pub(crate) use self::stream_support::{
    aborted_tool_results_if_needed, build_aborted_tool_results, build_tool_stream_callback,
    build_tools_end_payload,
};
pub(crate) use self::stream_support::{
    build_tool_result_metadata, consume_sse_stream, emit_stream_callbacks, execute_tool_lifecycle,
    parsed_stream_response_is_empty, AiStreamCallbacks, EMPTY_STREAM_RESPONSE_PARSE_ERROR,
};

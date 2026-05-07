mod request_support;
mod stream_support;
#[cfg(test)]
mod tests;

pub(crate) use self::request_support::{
    build_abort_token, build_ai_client_success_payload, build_assistant_message_metadata,
    handle_transient_retry,
    completion_failed_error,
    extract_response_id_from_metadata, extract_response_status_from_metadata,
    is_non_terminal_response_status, normalize_reasoning_effort, normalize_turn_id,
    persist_assistant_response_with_policy, persist_user_message_and_build_content_parts,
    read_error_response_text, send_bearer_json_request, should_persist_assistant_message,
    validate_request_payload_size, AssistantResponsePersistenceRequest,
};
#[cfg(test)]
pub(crate) use self::request_support::{
    await_with_optional_abort, is_response_parse_error, is_transient_network_error,
    is_transient_transport_or_parse_error,
};
#[cfg(test)]
pub(crate) use self::stream_support::drain_sse_json_events;
pub(crate) use self::stream_support::{
    build_tool_result_metadata, consume_sse_stream, emit_stream_callbacks,
    execute_tool_lifecycle, parsed_stream_response_is_empty, AiStreamCallbacks,
    EMPTY_STREAM_RESPONSE_PARSE_ERROR,
};
#[cfg(test)]
pub(crate) use self::stream_support::{
    aborted_tool_results_if_needed, build_aborted_tool_results, build_tool_stream_callback,
    build_tools_end_payload,
};

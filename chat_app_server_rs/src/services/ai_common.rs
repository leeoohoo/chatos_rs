mod request_support;
mod stream_support;
#[cfg(test)]
mod tests;

pub(crate) use self::request_support::{
    await_with_optional_abort, build_abort_token, build_assistant_message_metadata,
    build_bearer_post_request, build_user_content_parts, build_user_message_metadata,
    completion_failed_error, normalize_reasoning_effort, normalize_turn_id, truncate_log,
    validate_request_payload_size,
};
#[cfg(test)]
pub(crate) use self::stream_support::drain_sse_json_events;
pub(crate) use self::stream_support::{
    build_aborted_tool_results, build_tool_result_metadata, build_tool_stream_callback,
    consume_sse_stream,
};

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod request_support;
mod stream_support;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use self::request_support::await_with_optional_abort;
#[cfg(test)]
pub(crate) use self::request_support::persist_user_message_and_build_content_parts;
pub(crate) use self::request_support::{
    attach_ai_client_success_extra, build_ai_client_success_payload,
    build_assistant_message_metadata, build_user_content_parts, build_user_message_metadata,
    classify_user_facing_ai_error, normalize_task_runner_async_plan_metadata,
    normalize_task_runner_async_tool_call_metadata, normalize_turn_id,
    TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
#[cfg(test)]
pub(crate) use self::request_support::{
    build_abort_token, completion_failed_error, is_task_runner_async_plan_message_mode,
    should_persist_assistant_message, terminal_empty_response_error,
};
pub(crate) use self::stream_support::build_tool_result_metadata;
#[cfg(test)]
pub(crate) use self::stream_support::drain_sse_json_events;
#[cfg(test)]
pub(crate) use self::stream_support::{
    aborted_tool_results_if_needed, build_aborted_tool_results, build_tool_stream_callback,
    build_tools_end_payload, consume_sse_stream, emit_stream_callbacks,
    parsed_stream_response_is_empty,
};
#[cfg(test)]
pub(crate) use self::stream_support::{execute_tool_lifecycle, AiStreamCallbacks};

mod assistant_response;
mod error_classify;
mod request_transport;
mod user_message;

pub(crate) use self::assistant_response::{
    attach_ai_client_success_extra, build_ai_client_success_payload,
    build_assistant_message_metadata, completion_failed_error, is_non_terminal_response_status,
    is_task_runner_async_plan_message_mode, normalize_task_runner_async_plan_metadata,
    normalize_task_runner_async_tool_call_metadata, persist_assistant_response_with_policy,
    should_persist_assistant_message, terminal_empty_response_error,
    AssistantResponsePersistenceRequest, TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
pub(crate) use self::error_classify::{
    classify_user_facing_ai_error, handle_transient_retry, is_retryable_provider_backpressure_error,
};
pub(crate) use self::request_transport::build_abort_token;
#[cfg(test)]
pub(crate) use self::request_transport::{
    await_with_optional_abort, format_error_response, truncate_log,
};
pub(crate) use self::user_message::{
    build_user_content_parts, normalize_turn_id, persist_user_message_and_build_content_parts,
};

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod assistant_response;
mod error_classify;
#[cfg(test)]
mod request_transport;
mod user_message;

pub(crate) use self::assistant_response::{
    attach_ai_client_success_extra, build_ai_client_success_payload,
    build_assistant_message_metadata, normalize_task_runner_async_plan_metadata,
    normalize_task_runner_async_tool_call_metadata, TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
#[cfg(test)]
pub(crate) use self::assistant_response::{
    completion_failed_error, is_task_runner_async_plan_message_mode,
    should_persist_assistant_message, terminal_empty_response_error,
};
pub(crate) use self::error_classify::classify_user_facing_ai_error;
#[cfg(test)]
pub(crate) use self::request_transport::{
    await_with_optional_abort, build_abort_token, format_error_response, truncate_log,
};
#[cfg(test)]
pub(crate) use self::user_message::persist_user_message_and_build_content_parts;
pub(crate) use self::user_message::{
    build_user_content_parts, build_user_message_metadata, normalize_turn_id,
};

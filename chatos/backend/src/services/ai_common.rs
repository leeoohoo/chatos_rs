// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod request_support;
mod stream_support;
#[cfg(test)]
mod tests;

pub(crate) use self::request_support::{
    attach_ai_client_success_extra, build_ai_client_success_payload,
    build_assistant_message_metadata, build_user_content_parts, build_user_message_metadata,
    classify_user_facing_ai_error, normalize_task_runner_async_plan_metadata,
    normalize_task_runner_async_tool_call_metadata, normalize_turn_id,
    TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
pub(crate) use self::stream_support::build_tool_result_metadata;

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod compat;
pub mod error_policy;
pub mod input_transform;
pub mod model_config;
pub mod request;
pub mod request_payload;
pub mod request_retry;
pub mod response_parse;
pub mod simple_prompt;
pub mod stream;
pub mod stream_parse;
pub mod tool_call;
pub mod traits;

pub use compat::{
    cap_tool_output_for_input, extract_usage_snapshot, log_usage_snapshot,
    rewrite_system_messages_to_user, truncate_function_call_outputs_in_input, UsageSnapshot,
};
pub use error_policy::{
    classify_transient_retry, classify_user_facing_ai_error, exhausted_transient_retry_message,
    handle_transient_retry, handle_transient_retry_with_abort, is_context_length_exceeded_error,
    is_invalid_input_text_error, is_missing_tool_call_error, is_rate_limited_provider_error,
    is_request_body_too_large_error, is_response_parse_error,
    is_retryable_provider_backpressure_error, is_retryable_provider_overload_error,
    is_transient_network_error, is_transient_transport_or_parse_error,
    is_upstream_auth_unavailable_error, is_upstream_connection_interrupted_error,
    replay_request_error_policy, transient_retry_backoff_ms, transient_retry_kind_label,
    RequestErrorReplay, TransientRetryAction,
};
pub use input_transform::{
    append_input_items, assistant_visible_text, build_current_input_items, content_parts_to_text,
    convert_parts_to_response_input, extract_raw_input, normalize_input_for_provider,
    normalize_input_to_text_value, prepend_input_items, response_content_has_image_part,
    to_message_item, to_message_item_with_reasoning,
};
pub use request::{AiRequestHandler, AiRequestOptions, AiResponse, AiTransport, StreamCallbacks};
pub use simple_prompt::{
    base_url_disallows_system_messages, base_url_requires_responses_input_list,
    build_responses_text_input, is_input_must_be_list_error, is_system_messages_not_allowed_error,
    run_compatible_prompt_with, select_preferred_response_text, should_retry_transport_error,
    wrap_prompt_with_system_context, SimplePromptOptions,
};
pub use traits::{
    JsonSchemaOutputFormat, ModelRequest, ModelRuntimeConfig, DEFAULT_MODEL_REQUEST_MAX_RETRIES,
};

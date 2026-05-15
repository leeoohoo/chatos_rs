mod assistant_response;
mod error_classify;
mod request_transport;
mod user_message;

pub(crate) use self::assistant_response::{
    build_ai_client_success_payload, build_assistant_message_metadata, completion_failed_error,
    extract_response_id_from_metadata, extract_response_status_from_metadata,
    is_non_terminal_response_status, persist_assistant_response_with_policy,
    should_persist_assistant_message, terminal_empty_response_error,
    AssistantResponsePersistenceRequest,
};
pub(crate) use self::error_classify::handle_transient_retry;
pub(crate) use self::error_classify::is_retryable_provider_overload_error;
#[cfg(test)]
pub(crate) use self::error_classify::{
    is_response_parse_error, is_transient_network_error, is_transient_transport_or_parse_error,
};
#[cfg(test)]
pub(crate) use self::request_transport::{
    await_with_optional_abort, format_error_response, truncate_log,
};
pub(crate) use self::request_transport::{
    build_abort_token, normalize_reasoning_effort, read_error_response_text,
    send_bearer_json_request, validate_request_payload_size,
};
pub(crate) use self::user_message::{
    normalize_turn_id, persist_user_message_and_build_content_parts,
};

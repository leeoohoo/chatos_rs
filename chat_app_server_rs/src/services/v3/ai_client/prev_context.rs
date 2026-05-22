#[cfg(test)]
pub(super) use crate::services::ai_common::{
    is_response_parse_error, is_transient_network_error, is_transient_transport_or_parse_error,
};

pub(super) fn base_url_disallows_system_messages(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();

    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    if let Ok(value) = std::env::var("DISABLE_SYSTEM_MESSAGES_FOR_PROXY") {
        let normalized = value.trim().to_lowercase();
        if normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on" {
            return true;
        }
    }

    false
}

pub(super) fn is_invalid_input_text_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("input_text")
        && (message.contains("invalid value") || message.contains("invalid_value"))
}

pub(super) fn is_missing_tool_call_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("no tool call found")
        && (message.contains("function call output") || message.contains("function_call_output"))
}

pub(super) fn is_system_messages_not_allowed_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("system messages are not allowed")
}

pub(super) fn is_input_must_be_list_error(err: &str) -> bool {
    err.to_lowercase().contains("input must be a list")
}

pub(super) fn is_context_length_exceeded_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("context_length_exceeded")
        || message.contains("input exceeds the context window")
        || message.contains("maximum context length")
        || (message.contains("context window") && message.contains("exceed"))
}

pub(super) fn is_request_body_too_large_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("request body too large")
        || message.contains("body too large")
        || message.contains("payload too large")
}

#[cfg(test)]
mod tests {
    use super::{
        base_url_disallows_system_messages, is_context_length_exceeded_error,
        is_request_body_too_large_error, is_response_parse_error,
        is_system_messages_not_allowed_error, is_transient_network_error,
        is_transient_transport_or_parse_error,
    };

    #[test]
    fn detects_context_window_overflow_errors() {
        assert!(is_context_length_exceeded_error(
            "context_length_exceeded: input exceeds the context window"
        ));
        assert!(is_context_length_exceeded_error(
            "Your input exceeds the context window of this model"
        ));
        assert!(!is_context_length_exceeded_error("rate_limit_exceeded"));
    }

    #[test]
    fn detects_relay_domain_system_message_restriction() {
        assert!(base_url_disallows_system_messages(
            "https://relay.nf.video/v1"
        ));
        assert!(!base_url_disallows_system_messages(
            "https://api.openai.com/v1"
        ));
    }

    #[test]
    fn detects_system_message_not_allowed_errors() {
        assert!(is_system_messages_not_allowed_error(
            "{\"detail\":\"System messages are not allowed\"}"
        ));
        assert!(!is_system_messages_not_allowed_error("rate_limit_exceeded"));
    }

    #[test]
    fn detects_request_body_too_large_errors() {
        assert!(is_request_body_too_large_error(
            "Read from request Body failed: http: request body too large"
        ));
        assert!(is_request_body_too_large_error("payload too large"));
        assert!(!is_request_body_too_large_error("rate_limit_exceeded"));
    }

    #[test]
    fn detects_response_parse_errors() {
        assert!(is_response_parse_error(
            "invalid JSON response (status 200): expected value at line 1 column 1"
        ));
        assert!(is_response_parse_error(
            "stream response parse failed: no valid events parsed"
        ));
        assert!(!is_response_parse_error("status 401: unauthorized"));
    }

    #[test]
    fn detects_transient_network_errors() {
        assert!(is_transient_network_error(
            "error sending request for url (https://api.openai.com/v1/responses)"
        ));
        assert!(is_transient_network_error(
            "status 503: service unavailable"
        ));
        assert!(is_transient_network_error(
            "{\"error\":{\"message\":\"The engine is currently overloaded, please try again later\",\"type\":\"engine_overloaded_error\"}}"
        ));
        assert!(is_transient_network_error("request timed out"));
        assert!(!is_transient_network_error("status 401: invalid api key"));
    }

    #[test]
    fn combines_transient_network_and_parse_detection() {
        assert!(is_transient_transport_or_parse_error(
            "invalid JSON response (status 200): expected value"
        ));
        assert!(is_transient_transport_or_parse_error(
            "status 504: gateway timeout"
        ));
        assert!(!is_transient_transport_or_parse_error(
            "status 400: invalid_request_error"
        ));
    }
}

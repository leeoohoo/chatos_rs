pub(super) fn should_use_prev_id_for_next_turn(
    prefer_stateless: bool,
    can_use_prev_id: bool,
    has_next_response_id: bool,
) -> bool {
    !prefer_stateless && can_use_prev_id && has_next_response_id
}

pub(super) fn is_unsupported_previous_response_id_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("previous_response_id")
        && (message.contains("unsupported parameter") || message.contains("invalid parameter"))
}

pub(super) fn base_url_allows_prev(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();

    if url.contains("api.openai.com") {
        return true;
    }
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    if let Ok(value) = std::env::var("ALLOW_PREV_ID_FOR_PROXY") {
        let normalized = value.trim().to_lowercase();
        if normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on" {
            return true;
        }
    }

    false
}

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

pub(super) fn is_response_parse_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("invalid json response")
        || message.contains("stream response parse failed")
        || message.contains("error decoding response body")
        || message.contains("unexpected end of json input")
        || message.contains("eof while parsing")
}

pub(super) fn is_transient_network_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("error sending request for url")
        || message.contains("connection closed before message completed")
        || message.contains("connection reset")
        || message.contains("broken pipe")
        || message.contains("connection refused")
        || message.contains("network is unreachable")
        || message.contains("unexpected eof")
        || message.contains("timed out")
        || message.contains("timeout")
        || message.contains("dns error")
        || message.contains("temporary failure in name resolution")
        || message.contains("failed to lookup address information")
        || message.contains("status 408")
        || message.contains("status 502")
        || message.contains("status 503")
        || message.contains("status 504")
        || message.contains("status 522")
        || message.contains("status 523")
        || message.contains("status 524")
}

pub(super) fn is_transient_transport_or_parse_error(err: &str) -> bool {
    is_transient_network_error(err) || is_response_parse_error(err)
}

pub(super) fn reduce_history_limit(limit: i64) -> Option<i64> {
    if limit <= 1 {
        return None;
    }

    Some((limit / 2).max(1))
}

#[cfg(test)]
mod tests {
    use super::{
        base_url_disallows_system_messages, is_context_length_exceeded_error,
        is_request_body_too_large_error, is_response_parse_error,
        is_system_messages_not_allowed_error, is_transient_network_error,
        is_transient_transport_or_parse_error, reduce_history_limit,
        should_use_prev_id_for_next_turn,
    };

    #[test]
    fn keeps_stateless_mode_when_prefer_stateless_enabled() {
        assert!(!should_use_prev_id_for_next_turn(true, true, true));
        assert!(!should_use_prev_id_for_next_turn(true, true, false));
    }

    #[test]
    fn allows_prev_id_when_stateful_and_response_id_exists() {
        assert!(should_use_prev_id_for_next_turn(false, true, true));
        assert!(!should_use_prev_id_for_next_turn(false, true, false));
        assert!(!should_use_prev_id_for_next_turn(false, false, true));
    }

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
    fn reduce_history_limit_halves_until_one() {
        assert_eq!(reduce_history_limit(20), Some(10));
        assert_eq!(reduce_history_limit(3), Some(1));
        assert_eq!(reduce_history_limit(1), None);
        assert_eq!(reduce_history_limit(0), None);
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

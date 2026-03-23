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
        || message.contains("engine_overloaded_error")
        || message.contains("currently overloaded, please try again later")
        || message.contains("server is currently overloaded")
}

pub(super) fn is_transient_transport_or_parse_error(err: &str) -> bool {
    is_transient_network_error(err) || is_response_parse_error(err)
}

pub(super) fn cap_tool_content_for_input(raw: &str) -> String {
    truncate_text_keep_tail(raw, tool_content_item_max_chars())
}

fn tool_content_item_max_chars() -> usize {
    std::env::var("AI_V2_TOOL_OUTPUT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(8_000)
}

fn truncate_text_keep_tail(raw: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total = raw.chars().count();
    if total <= max_chars {
        return raw.to_string();
    }

    let marker = format!("[...truncated {} chars...]\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return raw
            .chars()
            .rev()
            .take(max_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }

    let keep_tail = max_chars - marker_chars;
    let tail: String = raw
        .chars()
        .rev()
        .take(keep_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}", marker, tail)
}

#[cfg(test)]
mod tests {
    use super::{
        cap_tool_content_for_input, is_response_parse_error, is_transient_network_error,
        is_transient_transport_or_parse_error,
    };

    #[test]
    fn cap_tool_content_for_input_truncates_large_text() {
        let text = "a".repeat(20_000);
        let truncated = cap_tool_content_for_input(text.as_str());
        assert!(truncated.len() < text.len());
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn cap_tool_content_for_input_keeps_short_text() {
        let text = "short output";
        assert_eq!(cap_tool_content_for_input(text), text.to_string());
    }

    #[test]
    fn detects_response_parse_errors() {
        assert!(is_response_parse_error(
            "invalid JSON response (status 200): expected value"
        ));
        assert!(is_response_parse_error(
            "stream response parse failed: no valid SSE events parsed from provider"
        ));
        assert!(!is_response_parse_error("status 401: unauthorized"));
    }

    #[test]
    fn detects_transient_network_errors() {
        assert!(is_transient_network_error(
            "error sending request for url (https://api.openai.com/v1/chat/completions)"
        ));
        assert!(is_transient_network_error(
            "status 503: service unavailable"
        ));
        assert!(is_transient_network_error(
            "{\"error\":{\"message\":\"The engine is currently overloaded, please try again later\",\"type\":\"engine_overloaded_error\"}}"
        ));
        assert!(!is_transient_network_error("status 401: invalid api key"));
    }

    #[test]
    fn combines_transient_network_and_parse_detection() {
        assert!(is_transient_transport_or_parse_error(
            "error decoding response body: unexpected eof"
        ));
        assert!(is_transient_transport_or_parse_error(
            "status 504: gateway timeout"
        ));
        assert!(!is_transient_transport_or_parse_error(
            "status 400: invalid_request_error"
        ));
    }
}

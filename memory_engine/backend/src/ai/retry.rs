// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Memory Engine-owned classification for its stateless summary requests.
//! This intentionally does not depend on the retired server Agent runtime.

pub(crate) fn is_transient_transport_or_parse_error(error: &str) -> bool {
    is_transient_network_error(error) || is_response_parse_error(error)
}

pub(crate) fn transient_retry_kind_label(error: &str) -> &'static str {
    if is_ai_request_timeout(error) {
        "模型响应连续无数据超时"
    } else if is_upstream_auth_unavailable(error) {
        "上游认证资源暂不可用"
    } else if is_upstream_connection_interrupted(error) {
        "上游连接在开始处理前中断"
    } else if is_response_parse_error(error) {
        "响应解析异常"
    } else if is_rate_limited(error) {
        "上游限流"
    } else if is_provider_overloaded(error) {
        "上游暂时过载"
    } else if is_retryable_gateway_error(error) {
        "上游服务暂不可用"
    } else {
        "网络波动"
    }
}

pub(crate) fn transient_retry_backoff_ms(error: &str, retry_count: usize) -> u64 {
    let retry_count = retry_count.max(1);
    let (base_ms, cap_ms) = if is_upstream_auth_unavailable(error)
        || is_rate_limited(error)
        || is_provider_overloaded(error)
    {
        (2_000_u64, 30_000_u64)
    } else if is_upstream_connection_interrupted(error) {
        (3_000_u64, 30_000_u64)
    } else if is_retryable_gateway_error(error) {
        (1_000_u64, 16_000_u64)
    } else if is_response_parse_error(error) {
        (2_000_u64, 30_000_u64)
    } else {
        (750_u64, 12_000_u64)
    };
    let exponent = u32::try_from(retry_count.saturating_sub(1))
        .unwrap_or(u32::MAX)
        .min(16);
    let exponential_ms = base_ms.saturating_mul(1_u64 << exponent).min(cap_ms);
    retry_after_hint_ms(error)
        .map(|hint_ms| exponential_ms.max(hint_ms.min(120_000)))
        .unwrap_or(exponential_ms)
}

fn is_transient_network_error(error: &str) -> bool {
    let message = error.to_lowercase();
    message.contains("ai request timed out")
        || message.contains("error sending request for url")
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
        || is_upstream_auth_unavailable(error)
        || is_upstream_connection_interrupted(error)
        || is_rate_limited(error)
        || is_provider_overloaded(error)
        || is_retryable_gateway_error(error)
}

fn is_response_parse_error(error: &str) -> bool {
    let message = error.to_lowercase();
    message.contains("invalid json response")
        || message.contains("stream response parse failed")
        || message.contains("stream response body failed")
        || message.contains("malformed sse event")
        || message.contains("error decoding response body")
        || message.contains("unexpected end of json input")
        || message.contains("eof while parsing")
        || message.contains("ai stream read failed")
        || message.contains("ai stream event utf-8 decode failed")
        || message.contains("ai stream expected content-type")
}

fn is_ai_request_timeout(error: &str) -> bool {
    let message = error.to_lowercase();
    message.contains("ai request timed out")
        || message.contains("ai transport error (kind=timeout)")
}

fn is_upstream_connection_interrupted(error: &str) -> bool {
    let message = error.to_lowercase();
    message.contains("connection closed before message completed")
        || message.contains("disconnect/reset before headers")
        || message.contains("upstream connect error")
        || message.contains("connection reset by peer")
        || message.contains("peer closed connection")
}

fn is_upstream_auth_unavailable(error: &str) -> bool {
    let message = error.to_lowercase();
    message.contains("auth_unavailable")
        || message.contains("no auth available")
        || message.contains("no available auth")
        || message.contains("no available account")
}

fn is_rate_limited(error: &str) -> bool {
    let message = error.to_lowercase();
    if message.contains("insufficient_quota")
        || message.contains("billing hard limit")
        || message.contains("exceeded your current quota")
    {
        return false;
    }
    message.contains("rate limit exceeded")
        || message.contains("rate limit reached")
        || message.contains("rate_limit_exceeded")
        || message.contains("too many requests")
        || message.contains("requests rate limit")
        || message.contains("status=429")
        || message.contains("status 429")
}

fn is_provider_overloaded(error: &str) -> bool {
    let message = error.to_lowercase();
    message.contains("engine_overloaded_error")
        || message.contains("server_is_overloaded")
        || message.contains("currently overloaded")
        || message.contains("model is at capacity")
        || message.contains("selected model is at capacity")
}

fn is_retryable_gateway_error(error: &str) -> bool {
    let message = error.to_lowercase();
    [408, 502, 503, 504, 522, 523, 524]
        .into_iter()
        .any(|status| {
            message.contains(format!("status {status}").as_str())
                || message.contains(format!("status={status}").as_str())
        })
        || message.contains("upstream connect error")
        || message.contains("disconnect/reset before headers")
}

fn retry_after_hint_ms(error: &str) -> Option<u64> {
    const MARKER: &str = "retry_after_ms=";
    let start = error.find(MARKER)? + MARKER.len();
    let digits = error[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!digits.is_empty())
        .then(|| digits.parse::<u64>().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_errors_emitted_by_the_memory_engine_transport() {
        for error in [
            "ai request timed out after 120s while waiting for stream data",
            "ai request failed: connection reset by peer",
            "ai request status=503 Service Unavailable endpoint=/v1/responses body=busy",
            "ai stream read failed: unexpected EOF",
            "ai stream event utf-8 decode failed: invalid utf-8 sequence",
        ] {
            assert!(
                is_transient_transport_or_parse_error(error),
                "expected transient classification for {error}"
            );
        }
    }

    #[test]
    fn authentication_and_invalid_requests_are_not_retried() {
        assert!(!is_transient_transport_or_parse_error(
            "ai request status=401 Unauthorized body=invalid api key"
        ));
        assert!(!is_transient_transport_or_parse_error(
            "ai request status=400 Bad Request body=invalid input"
        ));
    }

    #[test]
    fn backoff_is_bounded_and_honors_a_retry_after_hint() {
        assert_eq!(
            transient_retry_backoff_ms("status=503 retry_after_ms=9000", 1),
            9_000
        );
        assert_eq!(
            transient_retry_backoff_ms("status=503 retry_after_ms=999999", 1),
            120_000
        );
        assert_eq!(
            transient_retry_backoff_ms("connection reset", usize::MAX),
            12_000
        );
    }
}

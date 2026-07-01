// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::time::{sleep, Duration};
use tracing::warn;

const RATE_LIMITED_ERROR_CODE: &str = "RATE_LIMITED";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestErrorReplay {
    pub rebuild_stateless_on_missing_tool_call: bool,
    pub input_must_be_list: bool,
}

pub enum TransientRetryAction {
    Retry {
        retry_kind: &'static str,
        next_retry_count: usize,
        backoff_ms: u64,
    },
    Exhausted {
        error_message: String,
    },
}

pub fn is_invalid_input_text_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("input_text")
        && (message.contains("invalid value") || message.contains("invalid_value"))
}

pub fn is_missing_tool_call_error(err: &str) -> bool {
    let message = err.to_lowercase();
    (message.contains("no tool call found")
        && (message.contains("function call output") || message.contains("function_call_output")))
        || (message.contains("no tool output found")
            && (message.contains("function call") || message.contains("function_call")))
}

pub fn is_context_length_exceeded_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("context_length_exceeded")
        || message.contains("input exceeds the context window")
        || message.contains("maximum context length")
        || (message.contains("context window") && message.contains("exceed"))
}

pub fn is_request_body_too_large_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("request body too large")
        || message.contains("body too large")
        || message.contains("payload too large")
}

pub fn replay_request_error_policy(err_msg: &str) -> RequestErrorReplay {
    RequestErrorReplay {
        rebuild_stateless_on_missing_tool_call: is_missing_tool_call_error(err_msg),
        input_must_be_list: crate::simple_prompt::is_input_must_be_list_error(err_msg),
    }
}

pub fn is_response_parse_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("invalid json response")
        || message.contains("stream response parse failed")
        || message.contains("error decoding response body")
        || message.contains("unexpected end of json input")
        || message.contains("eof while parsing")
}

pub fn is_transient_network_error(err: &str) -> bool {
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
        || is_retryable_provider_backpressure_error(err)
}

pub fn is_retryable_provider_overload_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("engine_overloaded_error")
        || message.contains("server_is_overloaded")
        || message.contains("our servers are currently overloaded")
        || message.contains("server is currently overloaded")
        || message.contains("currently overloaded")
        || message.contains("selected model is at capacity")
        || message.contains("model is at capacity")
        || (message.contains("at capacity") && message.contains("try a different model"))
}

pub fn is_rate_limited_provider_error(err: &str) -> bool {
    let message = err.to_lowercase();
    if is_non_retryable_quota_error(message.as_str()) {
        return false;
    }

    message.contains("rate limit exceeded")
        || message.contains("rate limit reached")
        || message.contains("rate_limit_exceeded")
        || message.contains("too many requests")
        || message.contains("requests rate limit")
        || (message.contains("status 429") && message.contains("try again later"))
}

pub fn is_retryable_provider_backpressure_error(err: &str) -> bool {
    is_rate_limited_provider_error(err) || is_retryable_provider_overload_error(err)
}

pub fn classify_user_facing_ai_error(err: &str) -> Option<(&'static str, String)> {
    if is_rate_limited_provider_error(err) {
        return Some((
            RATE_LIMITED_ERROR_CODE,
            "请求过于频繁，触发了上游模型接口限流。请稍后再试；如果连续出现，可减少上下文、减少并发请求或切换模型。"
                .to_string(),
        ));
    }

    None
}

pub fn is_transient_transport_or_parse_error(err: &str) -> bool {
    is_transient_network_error(err) || is_response_parse_error(err)
}

pub fn transient_retry_kind_label(err: &str) -> &'static str {
    if is_response_parse_error(err) {
        "响应解析异常"
    } else if is_rate_limited_provider_error(err) {
        "上游限流"
    } else if is_retryable_provider_overload_error(err) {
        "上游暂时过载"
    } else {
        "网络波动"
    }
}

pub fn transient_retry_backoff_ms(err: &str, retry_count: usize) -> u64 {
    if is_rate_limited_provider_error(err) {
        1000_u64 * retry_count as u64
    } else {
        150_u64 * retry_count as u64
    }
}

pub fn exhausted_transient_retry_message(
    retry_kind: &str,
    max_transient_retries: usize,
    err: &str,
) -> String {
    format!(
        "AI 请求失败：{}，已重试 {} 次，最后错误：{}",
        retry_kind, max_transient_retries, err
    )
}

pub fn classify_transient_retry(
    err: &str,
    transient_retry_count: usize,
    max_transient_retries: usize,
) -> Option<TransientRetryAction> {
    if !is_transient_transport_or_parse_error(err) {
        return None;
    }

    let retry_kind = transient_retry_kind_label(err);
    if transient_retry_count < max_transient_retries {
        let next_retry_count = transient_retry_count + 1;
        return Some(TransientRetryAction::Retry {
            retry_kind,
            next_retry_count,
            backoff_ms: transient_retry_backoff_ms(err, next_retry_count),
        });
    }

    Some(TransientRetryAction::Exhausted {
        error_message: exhausted_transient_retry_message(retry_kind, max_transient_retries, err),
    })
}

pub async fn handle_transient_retry(
    log_prefix: &str,
    err: &str,
    transient_retry_count: &mut usize,
    max_transient_retries: usize,
) -> Result<bool, String> {
    let Some(action) = classify_transient_retry(err, *transient_retry_count, max_transient_retries)
    else {
        return Ok(false);
    };

    match action {
        TransientRetryAction::Retry {
            retry_kind,
            next_retry_count,
            backoff_ms,
        } => {
            *transient_retry_count = next_retry_count;
            warn!(
                "{} transient {} detected; retry {}/{} after {}ms: {}",
                log_prefix,
                retry_kind,
                *transient_retry_count,
                max_transient_retries,
                backoff_ms,
                err
            );
            sleep(Duration::from_millis(backoff_ms)).await;
            Ok(true)
        }
        TransientRetryAction::Exhausted { error_message } => Err(error_message),
    }
}

fn is_non_retryable_quota_error(message: &str) -> bool {
    message.contains("insufficient_quota")
        || message.contains("exceeded your current quota")
        || message.contains("billing")
        || message.contains("credit balance")
}

#[cfg(test)]
mod tests {
    use super::{
        classify_transient_retry, classify_user_facing_ai_error, exhausted_transient_retry_message,
        handle_transient_retry, is_context_length_exceeded_error, is_rate_limited_provider_error,
        is_request_body_too_large_error, is_response_parse_error,
        is_retryable_provider_backpressure_error, is_retryable_provider_overload_error,
        is_transient_network_error, is_transient_transport_or_parse_error,
        replay_request_error_policy, transient_retry_backoff_ms, transient_retry_kind_label,
        RequestErrorReplay, TransientRetryAction,
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
    fn detects_request_body_too_large_errors() {
        assert!(is_request_body_too_large_error(
            "Read from request Body failed: http: request body too large"
        ));
        assert!(is_request_body_too_large_error("payload too large"));
        assert!(!is_request_body_too_large_error("rate_limit_exceeded"));
    }

    #[test]
    fn replays_request_error_policy() {
        assert_eq!(
            replay_request_error_policy(
                "No tool call found for function call output in previous response",
            ),
            RequestErrorReplay {
                rebuild_stateless_on_missing_tool_call: true,
                input_must_be_list: false,
            }
        );
        assert_eq!(
            replay_request_error_policy("No tool output found for function call call_123.",),
            RequestErrorReplay {
                rebuild_stateless_on_missing_tool_call: true,
                input_must_be_list: false,
            }
        );
        assert_eq!(
            replay_request_error_policy("Bad Request: input must be a list"),
            RequestErrorReplay {
                rebuild_stateless_on_missing_tool_call: false,
                input_must_be_list: true,
            }
        );
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
        assert!(is_transient_network_error(
            "ai response failed: finish_reason=failed; provider_error=code=server_is_overloaded; message=Our servers are currently overloaded. Please try again later."
        ));
        assert!(is_transient_network_error(
            "ai response failed: finish_reason=failed; provider_error=message=Selected model is at capacity. Please try a different model."
        ));
        assert!(is_transient_network_error(
            "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\"}}"
        ));
        assert!(!is_transient_network_error("status 401: invalid api key"));
    }

    #[test]
    fn detects_retryable_provider_overload_errors() {
        assert!(is_retryable_provider_overload_error(
            "provider_error=code=server_is_overloaded"
        ));
        assert!(is_retryable_provider_overload_error(
            "Our servers are currently overloaded. Please try again later."
        ));
        assert!(is_retryable_provider_overload_error(
            "Selected model is at capacity. Please try a different model."
        ));
        assert!(!is_retryable_provider_overload_error(
            "status 400: invalid_request_error"
        ));
    }

    #[test]
    fn detects_retryable_provider_rate_limit_errors() {
        assert!(is_rate_limited_provider_error(
            "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\",\"type\":\"bad_response_status_code\",\"code\":\"bad_response_status_code\"}}"
        ));
        assert!(is_rate_limited_provider_error(
            "{\"error\":{\"message\":\"Requests rate limit exceeded\"}}"
        ));
        assert!(!is_rate_limited_provider_error(
            "{\"error\":{\"message\":\"insufficient_quota\"}}"
        ));
    }

    #[test]
    fn detects_retryable_backpressure_union() {
        assert!(is_retryable_provider_backpressure_error(
            "status 429 Too Many Requests: try again later"
        ));
        assert!(is_retryable_provider_backpressure_error(
            "Selected model is at capacity. Please try a different model."
        ));
        assert!(!is_retryable_provider_backpressure_error(
            "status 401: invalid api key"
        ));
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

    #[test]
    fn chooses_retry_labels_and_backoff() {
        assert_eq!(
            transient_retry_kind_label("error decoding response body: unexpected eof"),
            "响应解析异常"
        );
        assert_eq!(
            transient_retry_kind_label("status 503: service unavailable"),
            "网络波动"
        );
        assert_eq!(
            transient_retry_kind_label(
                "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\"}}",
            ),
            "上游限流"
        );
        assert_eq!(
            transient_retry_kind_label(
                "Selected model is at capacity. Please try a different model.",
            ),
            "上游暂时过载"
        );
        assert_eq!(
            transient_retry_backoff_ms("status 503: service unavailable", 2),
            300
        );
        assert_eq!(
            transient_retry_backoff_ms(
                "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\"}}",
                3,
            ),
            3000
        );
    }

    #[test]
    fn classifies_transient_retry_states() {
        let first = classify_transient_retry("status 503: service unavailable", 0, 5);
        match first {
            Some(TransientRetryAction::Retry {
                retry_kind,
                next_retry_count,
                backoff_ms,
            }) => {
                assert_eq!(retry_kind, "网络波动");
                assert_eq!(next_retry_count, 1);
                assert_eq!(backoff_ms, 150);
            }
            _ => panic!("expected retry action"),
        }

        let exhausted = classify_transient_retry("status 503: service unavailable", 5, 5);
        match exhausted {
            Some(TransientRetryAction::Exhausted { error_message }) => {
                assert_eq!(
                    error_message,
                    exhausted_transient_retry_message(
                        "网络波动",
                        5,
                        "status 503: service unavailable"
                    ),
                );
            }
            _ => panic!("expected exhausted action"),
        }

        assert!(classify_transient_retry("status 400: invalid_request_error", 0, 5).is_none());
    }

    #[tokio::test]
    async fn handle_transient_retry_returns_false_for_non_retryable_errors() {
        let mut retry_count = 0usize;
        let result = handle_transient_retry(
            "[test]",
            "status 400: invalid_request_error",
            &mut retry_count,
            5,
        )
        .await
        .expect("should not fail");

        assert!(!result);
        assert_eq!(retry_count, 0);
    }

    #[tokio::test]
    async fn handle_transient_retry_returns_exhausted_error_message() {
        let mut retry_count = 5usize;
        let err = handle_transient_retry(
            "[test]",
            "status 503: service unavailable",
            &mut retry_count,
            5,
        )
        .await
        .expect_err("should return exhausted error");

        assert!(err.contains("AI 请求失败"));
        assert!(err.contains("status 503: service unavailable"));
    }

    #[test]
    fn classifies_user_facing_rate_limit_errors() {
        let classified = classify_user_facing_ai_error(
            "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\"}}",
        )
        .expect("should classify rate limit");
        assert_eq!(classified.0, "RATE_LIMITED");
        assert!(classified.1.contains("请求过于频繁"));
    }
}

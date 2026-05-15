use tokio::time::{sleep, Duration};
use tracing::warn;

pub(crate) fn is_response_parse_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("invalid json response")
        || message.contains("stream response parse failed")
        || message.contains("error decoding response body")
        || message.contains("unexpected end of json input")
        || message.contains("eof while parsing")
}

pub(crate) fn is_transient_network_error(err: &str) -> bool {
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
        || is_retryable_provider_overload_error(err)
}

pub(crate) fn is_retryable_provider_overload_error(err: &str) -> bool {
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

pub(crate) fn is_transient_transport_or_parse_error(err: &str) -> bool {
    is_transient_network_error(err) || is_response_parse_error(err)
}

pub(crate) enum TransientRetryAction {
    Retry {
        retry_kind: &'static str,
        next_retry_count: usize,
        backoff_ms: u64,
    },
    Exhausted {
        error_message: String,
    },
}

pub(crate) fn transient_retry_kind_label(err: &str) -> &'static str {
    if is_response_parse_error(err) {
        "响应解析异常"
    } else if is_retryable_provider_overload_error(err) {
        "上游暂时过载"
    } else {
        "网络波动"
    }
}

pub(crate) fn transient_retry_backoff_ms(retry_count: usize) -> u64 {
    150_u64 * retry_count as u64
}

pub(crate) fn exhausted_transient_retry_message(
    retry_kind: &str,
    max_transient_retries: usize,
    err: &str,
) -> String {
    format!(
        "AI 请求失败：{}，已重试 {} 次，最后错误：{}",
        retry_kind, max_transient_retries, err
    )
}

pub(crate) fn classify_transient_retry(
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
            backoff_ms: transient_retry_backoff_ms(next_retry_count),
        });
    }

    Some(TransientRetryAction::Exhausted {
        error_message: exhausted_transient_retry_message(retry_kind, max_transient_retries, err),
    })
}

pub(crate) async fn handle_transient_retry(
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

#[cfg(test)]
mod tests {
    use super::{
        classify_transient_retry, exhausted_transient_retry_message, handle_transient_retry,
        is_response_parse_error, is_retryable_provider_overload_error,
        is_transient_network_error, is_transient_transport_or_parse_error,
        transient_retry_backoff_ms, transient_retry_kind_label, TransientRetryAction,
    };

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

    #[test]
    fn derives_retry_labels_and_backoff() {
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
                "ai response failed: finish_reason=failed; provider_error=message=Selected model is at capacity. Please try a different model."
            ),
            "上游暂时过载"
        );
        assert_eq!(transient_retry_backoff_ms(3), 450);
    }

    #[test]
    fn builds_exhausted_retry_message() {
        let message =
            exhausted_transient_retry_message("网络波动", 5, "status 503: service unavailable");
        assert!(message.contains("网络波动"));
        assert!(message.contains("已重试 5 次"));
        assert!(message.contains("status 503"));
    }

    #[test]
    fn classifies_retryable_and_exhausted_transient_errors() {
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
                assert!(error_message.contains("已重试 5 次"));
            }
            _ => panic!("expected exhausted action"),
        }

        assert!(classify_transient_retry("status 400: invalid_request_error", 0, 5).is_none());
    }

    #[tokio::test]
    async fn handle_transient_retry_returns_false_for_non_retryable_errors() {
        let mut retry_count = 0usize;
        let result = handle_transient_retry(
            "[TEST]",
            "status 400: invalid_request_error",
            &mut retry_count,
            5,
        )
        .await
        .expect("non-retryable path should not fail");

        assert!(!result);
        assert_eq!(retry_count, 0);
    }

    #[tokio::test]
    async fn handle_transient_retry_returns_exhausted_error_message() {
        let mut retry_count = 5usize;
        let err = handle_transient_retry(
            "[TEST]",
            "status 503: service unavailable",
            &mut retry_count,
            5,
        )
        .await
        .expect_err("retry should be exhausted");

        assert!(err.contains("已重试 5 次"));
        assert_eq!(retry_count, 5);
    }
}

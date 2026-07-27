// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::{SystemTime, UNIX_EPOCH};

use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::warn;

const RATE_LIMITED_ERROR_CODE: &str = "RATE_LIMITED";
const AUTH_INVALID_ERROR_CODE: &str = "AUTH_INVALID";

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
        || message.contains("stream response body failed")
        || message.contains("malformed sse event")
        || message.contains("error decoding response body")
        || message.contains("unexpected end of json input")
        || message.contains("eof while parsing")
}

/// A streaming response that failed while reading or parsing should not be
/// replayed in the same fragile mode. The runtime keeps non-stream mode for
/// the rest of the current model iteration once this recovery is activated.
pub fn should_retry_without_stream(err: &str) -> bool {
    is_response_parse_error(err)
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
        || message.contains("status=408")
        || message.contains("status 502")
        || message.contains("status=502")
        || message.contains("status 503")
        || message.contains("status=503")
        || message.contains("status 504")
        || message.contains("status=504")
        || message.contains("status 522")
        || message.contains("status=522")
        || message.contains("status 523")
        || message.contains("status=523")
        || message.contains("status 524")
        || message.contains("status=524")
        || message.contains("error code: 522")
        || message.contains("error code: 523")
        || message.contains("error code: 524")
        || is_upstream_auth_unavailable_error(err)
        || is_retryable_failed_provider_response(err)
        || is_retryable_provider_backpressure_error(err)
}

/// Detects failures where the HTTP connection ended before the provider
/// completed a response. Gateways commonly record these attempts with zero
/// input/output tokens because model processing never started.
pub fn is_upstream_connection_interrupted_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("connection closed before message completed")
        || message.contains("disconnect/reset before headers")
        || message.contains("upstream connect error")
        || message.contains("connection reset by peer")
        || message.contains("peer closed connection")
}

/// Detects a gateway's temporary lack of provider-side credentials/accounts.
/// This is distinct from a user's invalid API key, which is normally a 401 and
/// must not be retried.
pub fn is_upstream_auth_unavailable_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("auth_unavailable")
        || message.contains("no auth available")
        || message.contains("no available auth")
        || message.contains("no available account")
}

pub fn is_retryable_failed_provider_response(err: &str) -> bool {
    let message = err.to_lowercase();
    if !message.contains("ai response failed: finish_reason=failed") {
        return false;
    }
    if is_provider_authentication_error(err) || is_non_retryable_quota_error(message.as_str()) {
        return false;
    }
    !message.contains("invalid_request_error")
        && !message.contains("invalid request")
        && !message.contains("bad_request")
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

pub fn is_provider_authentication_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("status 401")
        || message.contains("unauthorized")
        || message.contains("invalid token")
        || message.contains("invalid api key")
        || message.contains("incorrect api key")
}

pub fn classify_user_facing_ai_error(err: &str) -> Option<(&'static str, String)> {
    if is_provider_authentication_error(err) {
        return Some((
            AUTH_INVALID_ERROR_CODE,
            "\u{6a21}\u{578b}\u{670d}\u{52a1}\u{8ba4}\u{8bc1}\u{5931}\u{8d25}\u{ff1a}API Key/Token \u{65e0}\u{6548}\u{6216}\u{5df2}\u{8fc7}\u{671f}\u{ff0c}\u{8bf7}\u{5728}\u{6a21}\u{578b}\u{914d}\u{7f6e}\u{4e2d}\u{66f4}\u{65b0}\u{5bc6}\u{94a5}\u{540e}\u{91cd}\u{8bd5}\u{3002}"
                .to_string(),
        ));
    }

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
    if is_upstream_auth_unavailable_error(err) {
        "上游认证资源暂不可用"
    } else if is_upstream_connection_interrupted_error(err) {
        "上游连接在开始处理前中断"
    } else if is_response_parse_error(err) {
        "响应解析异常"
    } else if is_rate_limited_provider_error(err) {
        "上游限流"
    } else if is_retryable_provider_overload_error(err) {
        "上游暂时过载"
    } else if is_retryable_gateway_error(err) {
        "上游服务暂不可用"
    } else {
        "网络波动"
    }
}

pub fn transient_retry_backoff_ms(err: &str, retry_count: usize) -> u64 {
    let retry_count = retry_count.max(1);
    let (base_ms, cap_ms) = if is_upstream_auth_unavailable_error(err)
        || is_rate_limited_provider_error(err)
        || is_retryable_provider_overload_error(err)
    {
        (2_000_u64, 30_000_u64)
    } else if is_upstream_connection_interrupted_error(err) {
        // A request that failed before model processing often leaves the
        // gateway/provider route unhealthy for several seconds. Avoid sending
        // a burst of zero-token retries into the same failure window.
        (3_000_u64, 30_000_u64)
    } else if is_retryable_gateway_error(err) {
        (1_000_u64, 16_000_u64)
    } else if is_response_parse_error(err) {
        (2_000_u64, 30_000_u64)
    } else if is_retryable_failed_provider_response(err) {
        (250_u64, 4_000_u64)
    } else {
        (750_u64, 12_000_u64)
    };
    let exponent = u32::try_from(retry_count.saturating_sub(1))
        .unwrap_or(u32::MAX)
        .min(16);
    let exponential_ms = base_ms.saturating_mul(1_u64 << exponent).min(cap_ms);
    retry_after_hint_ms(err)
        .map(|hint_ms| exponential_ms.max(hint_ms.min(120_000)))
        .unwrap_or(exponential_ms)
}

fn retry_after_hint_ms(err: &str) -> Option<u64> {
    const MARKER: &str = "retry_after_ms=";
    let start = err.find(MARKER)? + MARKER.len();
    let digits = err[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!digits.is_empty())
        .then(|| digits.parse::<u64>().ok())
        .flatten()
}

fn is_retryable_gateway_error(err: &str) -> bool {
    let message = err.to_lowercase();
    [502, 503, 504, 522, 523, 524].into_iter().any(|status| {
        message.contains(format!("status {status}").as_str())
            || message.contains(format!("status={status}").as_str())
    }) || message.contains("upstream connect error")
        || message.contains("disconnect/reset before headers")
}

fn jittered_transient_retry_backoff_ms(err: &str, retry_count: usize) -> u64 {
    let backoff_ms = transient_retry_backoff_ms(err, retry_count);
    let jitter_window_ms = (backoff_ms / 5).max(1);
    let time_entropy = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as u64)
        .unwrap_or_default();
    let error_entropy = err.bytes().fold(retry_count as u64, |hash, byte| {
        hash.wrapping_mul(1_099_511_628_211)
            .wrapping_add(byte as u64)
    });
    backoff_ms.saturating_add((time_entropy ^ error_entropy) % (jitter_window_ms + 1))
}

pub fn exhausted_transient_retry_message(
    retry_kind: &str,
    max_transient_retries: usize,
    err: &str,
) -> String {
    if is_response_parse_error(err) {
        let detail = sanitized_response_parse_failure_detail(err);
        return format!(
            "AI 请求失败：{}，已重试 {} 次。{}。",
            retry_kind, max_transient_retries, detail
        );
    }
    format!(
        "AI 请求失败：{}，已重试 {} 次，最后错误：{}",
        retry_kind, max_transient_retries, err
    )
}

fn sanitized_response_parse_failure_detail(err: &str) -> &'static str {
    let message = err.to_lowercase();
    if message.contains("timed out") || message.contains("timeout") {
        "最后一次失败为上游响应读取超时"
    } else if message.contains("malformed sse event") {
        "最后一次响应包含无法解析的数据"
    } else if message.contains("no valid sse events") {
        "最后一次请求未收到可解析的上游响应事件"
    } else {
        "最后一次上游响应在传输或解码过程中中断"
    }
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
            backoff_ms: jittered_transient_retry_backoff_ms(err, next_retry_count),
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
    handle_transient_retry_with_abort(
        log_prefix,
        err,
        transient_retry_count,
        max_transient_retries,
        None,
    )
    .await
}

pub async fn handle_transient_retry_with_abort(
    log_prefix: &str,
    err: &str,
    transient_retry_count: &mut usize,
    max_transient_retries: usize,
    abort_token: Option<&CancellationToken>,
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
            let backoff = sleep(Duration::from_millis(backoff_ms));
            tokio::pin!(backoff);
            if let Some(token) = abort_token {
                tokio::select! {
                    _ = token.cancelled() => return Err("aborted".to_string()),
                    _ = &mut backoff => {}
                }
            } else {
                backoff.await;
            }
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
        handle_transient_retry, handle_transient_retry_with_abort,
        is_context_length_exceeded_error, is_provider_authentication_error,
        is_rate_limited_provider_error, is_request_body_too_large_error, is_response_parse_error,
        is_retryable_failed_provider_response, is_retryable_provider_backpressure_error,
        is_retryable_provider_overload_error, is_transient_network_error,
        is_transient_transport_or_parse_error, is_upstream_auth_unavailable_error,
        is_upstream_connection_interrupted_error, replay_request_error_policy,
        should_retry_without_stream, transient_retry_backoff_ms, transient_retry_kind_label,
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
        assert!(is_response_parse_error(
            "stream response body failed after 3 valid events: operation timed out"
        ));
        assert!(should_retry_without_stream(
            "stream response body failed: error decoding response body"
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
            "ai response failed: finish_reason=failed; provider_error=unavailable"
        ));
        assert!(is_transient_network_error(
            "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\"}}"
        ));
        assert!(!is_transient_network_error("status 401: invalid api key"));
    }

    #[test]
    fn detects_upstream_connections_that_end_before_processing() {
        assert!(is_upstream_connection_interrupted_error(
            "connection closed before message completed"
        ));
        assert!(is_upstream_connection_interrupted_error(
            "upstream connect error or disconnect/reset before headers"
        ));
        assert!(is_upstream_connection_interrupted_error(
            "connection reset by peer"
        ));
        assert!(!is_upstream_connection_interrupted_error(
            "status 503: service unavailable"
        ));
    }

    #[test]
    fn retries_failed_provider_responses_unless_the_error_is_actionable() {
        assert!(is_retryable_failed_provider_response(
            "ai response failed: finish_reason=failed; provider_error=unavailable"
        ));
        assert!(is_retryable_failed_provider_response(
            "ai response failed: finish_reason=failed; provider_error=type=server_error; message=temporary failure"
        ));
        assert!(!is_retryable_failed_provider_response(
            "ai response failed: finish_reason=failed; provider_error=type=invalid_request_error; message=invalid request"
        ));
        assert!(!is_retryable_failed_provider_response(
            "ai response failed: finish_reason=failed; provider_error=code=insufficient_quota; message=credit balance exhausted"
        ));
        assert!(!is_retryable_failed_provider_response(
            "ai response failed: finish_reason=failed; provider_error=message=invalid api key"
        ));
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
    fn distinguishes_upstream_auth_pool_outage_from_invalid_user_credentials() {
        let error = "status 503 Service Unavailable: auth_unavailable: no auth available (providers=codex, model=gpt-5.4)";
        assert!(is_upstream_auth_unavailable_error(error));
        assert!(is_transient_network_error(error));
        assert!(!is_provider_authentication_error(error));
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
    fn detects_provider_authentication_errors() {
        assert!(is_provider_authentication_error(
            "status 401 Unauthorized: {\"error\":{\"message\":\"Invalid token\"}}"
        ));
        assert!(is_provider_authentication_error("invalid api key"));
        assert!(!is_provider_authentication_error(
            "status 429 Too Many Requests"
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
            transient_retry_kind_label("connection closed before message completed"),
            "上游连接在开始处理前中断"
        );
        assert_eq!(
            transient_retry_kind_label("status 503: service unavailable"),
            "上游服务暂不可用"
        );
        assert_eq!(
            transient_retry_kind_label(
                "status 503: auth_unavailable: no auth available (providers=codex)"
            ),
            "上游认证资源暂不可用"
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
            2000
        );
        assert_eq!(
            transient_retry_backoff_ms(
                "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\"}}",
                3,
            ),
            8000
        );
        assert_eq!(
            transient_retry_backoff_ms(
                "status 503: auth_unavailable: no auth available (providers=codex)",
                5,
            ),
            30000
        );
        assert_eq!(
            transient_retry_backoff_ms("status 503 [retry_after_ms=45000]: service unavailable", 2,),
            45000
        );
        let interrupted = "connection closed before message completed";
        assert_eq!(transient_retry_backoff_ms(interrupted, 1), 3000);
        assert_eq!(transient_retry_backoff_ms(interrupted, 2), 6000);
        assert_eq!(transient_retry_backoff_ms(interrupted, 3), 12000);
        assert_eq!(transient_retry_backoff_ms(interrupted, 4), 24000);
        assert_eq!(transient_retry_backoff_ms(interrupted, 5), 30000);
        assert_eq!(
            transient_retry_backoff_ms(
                "stream response body failed: error decoding response body",
                1,
            ),
            2000
        );
        assert_eq!(
            transient_retry_backoff_ms(
                "stream response body failed: error decoding response body",
                5,
            ),
            30000
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
                assert_eq!(retry_kind, "上游服务暂不可用");
                assert_eq!(next_retry_count, 1);
                assert!((1000..=1200).contains(&backoff_ms));
            }
            _ => panic!("expected retry action"),
        }

        let exhausted = classify_transient_retry("status 503: service unavailable", 5, 5);
        match exhausted {
            Some(TransientRetryAction::Exhausted { error_message }) => {
                assert_eq!(
                    error_message,
                    exhausted_transient_retry_message(
                        "上游服务暂不可用",
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

    #[tokio::test]
    async fn retry_backoff_can_be_cancelled_immediately() {
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let mut retry_count = 0usize;
        let err = handle_transient_retry_with_abort(
            "[test]",
            "status 503: auth_unavailable: no auth available",
            &mut retry_count,
            5,
            Some(&token),
        )
        .await
        .expect_err("cancelled backoff should abort");

        assert_eq!(err, "aborted");
        assert_eq!(retry_count, 1);
    }

    #[test]
    fn classifies_user_facing_auth_errors() {
        let classified = classify_user_facing_ai_error(
            "status 401 Unauthorized: {\"error\":{\"message\":\"Invalid token\"}}",
        )
        .expect("should classify auth error");
        assert_eq!(classified.0, "AUTH_INVALID");
        assert!(classified.1.contains("API Key/Token"));
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

    #[test]
    fn exhausted_parse_error_keeps_sanitized_failure_class() {
        let message = exhausted_transient_retry_message(
            "响应解析异常",
            5,
            "AI transport error (kind=decode): error decoding response body",
        );
        assert!(message.contains("上游响应在传输或解码过程中中断"));
        assert!(!message.contains("error decoding response body"));

        let timeout_message = exhausted_transient_retry_message(
            "响应解析异常",
            5,
            "stream response body failed: operation timed out",
        );
        assert!(timeout_message.contains("上游响应读取超时"));
    }
}

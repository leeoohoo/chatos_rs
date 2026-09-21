#[cfg(test)]
mod tests {
    use super::{
        classify_transient_retry, classify_user_facing_ai_error, effective_transient_retry_limit,
        exhausted_transient_retry_message, handle_transient_retry,
        handle_transient_retry_with_abort, is_ai_request_timeout_error,
        is_context_length_exceeded_error, is_provider_authentication_error,
        is_rate_limited_provider_error, is_request_body_too_large_error, is_response_parse_error,
        is_retryable_failed_provider_response, is_retryable_provider_backpressure_error,
        is_retryable_provider_overload_error, is_transient_network_error,
        is_transient_transport_or_parse_error, is_upstream_auth_unavailable_error,
        is_upstream_connection_interrupted_error, replay_request_error_policy,
        transient_retry_backoff_ms, transient_retry_kind_label, RequestErrorReplay,
        TransientRetryAction,
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
            "incomplete Responses SSE stream: ended after 925 valid event(s) without response.completed, response.incomplete, or response.failed"
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

    #[test]
    fn request_inactivity_timeout_is_limited_to_one_retry() {
        let error = "AI transport error (kind=timeout): operation timed out";
        assert!(is_ai_request_timeout_error(error));
        assert_eq!(effective_transient_retry_limit(error, 5), 1);

        assert!(matches!(
            classify_transient_retry(error, 0, 5),
            Some(TransientRetryAction::Retry {
                next_retry_count: 1,
                ..
            })
        ));
        match classify_transient_retry(error, 1, 5) {
            Some(TransientRetryAction::Exhausted { error_message }) => {
                assert!(error_message.contains("模型响应连续无数据超时"));
                assert!(error_message.contains("已自动重试 1 次"));
            }
            _ => panic!("second inactivity timeout should exhaust retries"),
        }
    }

    #[test]
    fn explicit_gateway_timeout_keeps_configured_retry_limit() {
        let error = "status 504: gateway timeout";
        assert!(!is_ai_request_timeout_error(error));
        assert_eq!(effective_transient_retry_limit(error, 5), 5);
        assert!(matches!(
            classify_transient_retry(error, 1, 5),
            Some(TransientRetryAction::Retry {
                next_retry_count: 2,
                ..
            })
        ));
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

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::error_policy::{handle_transient_retry, is_transient_transport_or_parse_error};
use crate::{
    AiRequestHandler, AiResponse, ModelRuntimeConfig, StreamCallbacks,
    DEFAULT_MODEL_REQUEST_MAX_RETRIES,
};

#[derive(Clone, Default)]
pub struct SimplePromptOptions {
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub max_transient_retries: Option<usize>,
    /// Deprecated compatibility field. Prefer `max_transient_retries`, whose
    /// value is the number of retries after the initial request.
    pub max_attempts: Option<usize>,
    pub callbacks: StreamCallbacks,
}

pub async fn run_compatible_prompt_with<F>(
    handler: &AiRequestHandler,
    config: &ModelRuntimeConfig,
    user_prompt: &str,
    options: SimplePromptOptions,
    mut build_input: F,
) -> Result<AiResponse, String>
where
    F: FnMut(&str, bool) -> Value,
{
    let normalized_system_prompt = normalize_optional_text(
        options
            .system_prompt
            .as_deref()
            .or(config.instructions.as_deref()),
    );
    let mut no_system_messages = base_url_disallows_system_messages(config.base_url.as_str());
    let mut input_as_list = base_url_requires_responses_input_list(config.base_url.as_str());
    let max_transient_retries = options
        .max_transient_retries
        .or(config.max_transient_retries)
        .or_else(|| options.max_attempts.map(|attempts| attempts.max(1) - 1))
        .unwrap_or(DEFAULT_MODEL_REQUEST_MAX_RETRIES);
    let mut transient_retry_count = 0usize;

    loop {
        let wrapped_user_prompt = if no_system_messages {
            wrap_prompt_with_system_context(user_prompt, normalized_system_prompt.as_deref(), true)
        } else {
            user_prompt.to_string()
        };
        let instructions = if no_system_messages {
            None
        } else {
            normalized_system_prompt.clone()
        };
        let input = build_input(wrapped_user_prompt.as_str(), input_as_list);

        match handler
            .handle_request(
                config.base_url.as_str(),
                config.api_key.as_str(),
                input,
                config.supports_responses,
                config.model.clone(),
                instructions,
                None,
                options.temperature.or(config.temperature),
                options.max_output_tokens.or(config.max_output_tokens),
                options.callbacks.clone(),
                Some(config.provider.clone()),
                config.thinking_level.clone(),
                None,
            )
            .await
        {
            Ok(response) => return Ok(response),
            Err(err) => {
                if !input_as_list && is_input_must_be_list_error(err.as_str()) {
                    input_as_list = true;
                    continue;
                }
                if !no_system_messages && is_system_messages_not_allowed_error(err.as_str()) {
                    no_system_messages = true;
                    continue;
                }
                if handle_transient_retry(
                    "simple prompt request",
                    err.as_str(),
                    &mut transient_retry_count,
                    max_transient_retries,
                )
                .await?
                {
                    continue;
                }
                return Err(err);
            }
        }
    }
}

pub fn wrap_prompt_with_system_context(
    user_prompt: &str,
    system_prompt: Option<&str>,
    inline_system_context: bool,
) -> String {
    if !inline_system_context {
        return user_prompt.to_string();
    }

    let Some(system_prompt) = normalize_optional_text(system_prompt) else {
        return user_prompt.to_string();
    };

    format!("【系统上下文】\n{}\n\n{}", system_prompt, user_prompt)
}

pub fn build_responses_text_input(user_prompt: &str, input_as_list: bool) -> Value {
    if !input_as_list {
        return Value::String(user_prompt.to_string());
    }

    json!([
        {
            "type": "message",
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": user_prompt
                }
            ]
        }
    ])
}

pub fn base_url_disallows_system_messages(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    env_flag_enabled("DISABLE_SYSTEM_MESSAGES_FOR_PROXY")
}

pub fn base_url_requires_responses_input_list(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    env_flag_enabled("FORCE_RESPONSES_INPUT_LIST")
}

pub fn is_system_messages_not_allowed_error(err: &str) -> bool {
    err.to_lowercase()
        .contains("system messages are not allowed")
}

pub fn is_input_must_be_list_error(err: &str) -> bool {
    err.to_lowercase().contains("input must be a list")
}

pub fn should_retry_transport_error(err: &str) -> bool {
    is_transient_transport_or_parse_error(err)
}

pub fn select_preferred_response_text<'a>(
    content: &'a str,
    reasoning: Option<&'a str>,
) -> Option<&'a str> {
    if text_has_content(content) {
        return Some(content);
    }

    reasoning.filter(|value| text_has_content(value))
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn text_has_content(value: &str) -> bool {
    !value.trim().is_empty()
}

fn env_flag_enabled(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use serde_json::{json, Value};

    use super::{
        base_url_disallows_system_messages, build_responses_text_input,
        is_input_must_be_list_error, is_system_messages_not_allowed_error,
        run_compatible_prompt_with, select_preferred_response_text, should_retry_transport_error,
        wrap_prompt_with_system_context, SimplePromptOptions,
    };
    use crate::{AiRequestHandler, ModelRuntimeConfig};

    #[derive(Clone)]
    struct RetryProviderState {
        attempts: Arc<AtomicUsize>,
        failures_before_success: usize,
        failure_status: StatusCode,
    }

    async fn retry_provider(
        State(state): State<RetryProviderState>,
        Json(_payload): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let attempt = state.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt <= state.failures_before_success {
            return (
                state.failure_status,
                Json(json!({"error": {"message": "temporarily unavailable"}})),
            );
        }

        (
            StatusCode::OK,
            Json(json!({
                "id": "response-test",
                "status": "completed",
                "output_text": "ok"
            })),
        )
    }

    async fn start_retry_provider(
        failures_before_success: usize,
        failure_status: StatusCode,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let attempts = Arc::new(AtomicUsize::new(0));
        let state = RetryProviderState {
            attempts: Arc::clone(&attempts),
            failures_before_success,
            failure_status,
        };
        let app = Router::new()
            .route("/responses", post(retry_provider))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind retry provider");
        let address = listener.local_addr().expect("retry provider address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{address}"), attempts, server)
    }

    fn retry_test_config(base_url: String) -> ModelRuntimeConfig {
        ModelRuntimeConfig::openai_compatible(base_url, "test-key", "test-model", "openai")
            .with_responses_support(true)
    }

    #[test]
    fn detects_relay_domain_prompt_compat_rules() {
        assert!(base_url_disallows_system_messages(
            "https://relay.nf.video/v1"
        ));
        assert!(!base_url_disallows_system_messages(
            "https://api.openai.com/v1"
        ));
    }

    #[test]
    fn builds_text_input_for_responses_transport() {
        assert_eq!(
            build_responses_text_input("hello", false),
            Value::String("hello".to_string())
        );
        assert_eq!(
            build_responses_text_input("hello", true),
            json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": "hello"
                        }
                    ]
                }
            ])
        );
    }

    #[test]
    fn wraps_user_prompt_with_system_context_when_needed() {
        assert_eq!(
            wrap_prompt_with_system_context("User", Some("System"), true),
            "【系统上下文】\nSystem\n\nUser"
        );
        assert_eq!(
            wrap_prompt_with_system_context("User", Some("System"), false),
            "User"
        );
    }

    #[test]
    fn detects_prompt_compat_error_shapes() {
        assert!(is_input_must_be_list_error(
            "Bad Request: input must be a list"
        ));
        assert!(is_system_messages_not_allowed_error(
            "{\"detail\":\"System messages are not allowed\"}"
        ));
        assert!(should_retry_transport_error("request timed out"));
        assert!(should_retry_transport_error(
            "error sending request for url (https://newapi.example/v1/responses)"
        ));
        assert!(should_retry_transport_error(
            "status 503: service unavailable"
        ));
        assert!(!should_retry_transport_error("status 401: invalid api key"));
        assert!(!should_retry_transport_error(
            "status 400: invalid_request_error"
        ));
        assert!(!should_retry_transport_error(
            "insufficient_quota: credit balance exhausted"
        ));
    }

    #[tokio::test]
    async fn retries_transient_failures_until_request_succeeds() {
        let (base_url, attempts, server) =
            start_retry_provider(2, StatusCode::SERVICE_UNAVAILABLE).await;
        let response = run_compatible_prompt_with(
            &AiRequestHandler::new(),
            &retry_test_config(base_url),
            "hello",
            SimplePromptOptions {
                max_transient_retries: Some(3),
                ..Default::default()
            },
            build_responses_text_input,
        )
        .await
        .expect("transient request should recover");
        server.abort();

        assert_eq!(response.content, "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn reports_retry_exhaustion_after_configured_attempts() {
        let (base_url, attempts, server) =
            start_retry_provider(10, StatusCode::SERVICE_UNAVAILABLE).await;
        let error = run_compatible_prompt_with(
            &AiRequestHandler::new(),
            &retry_test_config(base_url).with_max_transient_retries(Some(2)),
            "hello",
            SimplePromptOptions::default(),
            build_responses_text_input,
        )
        .await
        .expect_err("transient request should exhaust retries");
        server.abort();

        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert!(error.contains("已重试 2 次"));
        assert!(error.contains("status 503"));
    }

    #[tokio::test]
    async fn does_not_retry_authentication_failures() {
        let (base_url, attempts, server) = start_retry_provider(10, StatusCode::UNAUTHORIZED).await;
        let error = run_compatible_prompt_with(
            &AiRequestHandler::new(),
            &retry_test_config(base_url),
            "hello",
            SimplePromptOptions {
                max_transient_retries: Some(5),
                ..Default::default()
            },
            build_responses_text_input,
        )
        .await
        .expect_err("authentication failure should not retry");
        server.abort();

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(error.contains("status 401"));
    }

    #[test]
    fn selects_visible_content_before_reasoning() {
        assert_eq!(
            select_preferred_response_text("answer", Some("thinking")),
            Some("answer")
        );
        assert_eq!(
            select_preferred_response_text("   ", Some("thinking")),
            Some("thinking")
        );
        assert_eq!(select_preferred_response_text("   ", Some("   ")), None);
    }
}

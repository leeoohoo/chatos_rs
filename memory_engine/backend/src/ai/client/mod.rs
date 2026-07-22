// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod config;
mod request;
mod responses;

use std::time::Instant;

use reqwest::Client;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::EngineModelProfile;

use self::config::build_client_config;
use self::request::send_text_request;
use self::responses::{build_user_prompt, request_kind, validate_summary_text};
use super::protocol::effective_request_temperature;

pub(crate) const SUMMARY_SYSTEM_PROMPT: &str = "You summarize conversation increments for a memory engine. Produce a concise, high-signal summary with concrete user intent, assistant response, and notable constraints. Do not use markdown bullets unless useful.";

#[derive(Clone)]
pub struct AiClient {
    http: Client,
    api_key: Option<String>,
    base_url: String,
    model: String,
    temperature: f64,
    timeout_secs: u64,
    supports_responses: bool,
    disable_thinking: bool,
    max_transient_retries: usize,
}

impl AiClient {
    pub fn new_with_profile(
        config: &AppConfig,
        profile: Option<&EngineModelProfile>,
    ) -> Result<Self, String> {
        build_client_config(config, profile)
    }

    pub fn is_enabled(&self) -> bool {
        self.api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    }

    pub async fn summarize(
        &self,
        title: Option<&str>,
        input: &str,
        max_tokens: Option<i64>,
    ) -> Result<String, String> {
        let user_prompt = build_user_prompt(title, input);
        self.generate_text(
            SUMMARY_SYSTEM_PROMPT,
            user_prompt.as_str(),
            max_tokens,
            Some(input.chars().count()),
            title
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some(),
        )
        .await
    }

    pub async fn generate_text(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: Option<i64>,
        input_chars: Option<usize>,
        title_present: bool,
    ) -> Result<String, String> {
        let api_key = self
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing MEMORY_ENGINE_OPENAI_API_KEY".to_string())?;
        let started_at = Instant::now();
        let requested_max_tokens = max_tokens.map(|value| value.clamp(128, 4000));
        let requested_max_tokens_label = requested_max_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "provider_default".to_string());
        let input_chars = input_chars.unwrap_or_else(|| user_prompt.chars().count());
        let effective_temperature = effective_request_temperature(
            self.base_url.as_str(),
            self.model.as_str(),
            self.temperature,
        );
        let request_kind = request_kind(self.supports_responses);
        info!(
            "[MEMORY-ENGINE-AI] request-start model={} base_url={} request_kind={} timeout_secs={} max_tokens={} input_chars={} title_present={} requested_temperature={} effective_temperature={} disable_thinking={}",
            self.model,
            self.base_url,
            request_kind,
            self.timeout_secs,
            requested_max_tokens_label,
            input_chars,
            title_present,
            self.temperature,
            effective_temperature,
            self.disable_thinking
        );

        let mut retry_count = 0usize;
        let text = loop {
            match send_text_request(
                self,
                api_key,
                system_prompt,
                user_prompt,
                requested_max_tokens,
                effective_temperature,
            )
            .await
            {
                Ok(text) => break text,
                Err(err) => {
                    if let Some(backoff_ms) =
                        transient_retry_backoff_ms(&err, retry_count, self.max_transient_retries)
                    {
                        retry_count += 1;
                        warn!(
                            "[MEMORY-ENGINE-AI] transient-retry model={} base_url={} request_kind={} retry={}/{} backoff_ms={} error={}",
                            self.model,
                            self.base_url,
                            request_kind,
                            retry_count,
                            self.max_transient_retries,
                            backoff_ms,
                            err
                        );
                        sleep(Duration::from_millis(backoff_ms)).await;
                        continue;
                    }

                    let log_label = if err.contains("timed out") {
                        "request-timeout"
                    } else {
                        "request-failed"
                    };
                    warn!(
                        "[MEMORY-ENGINE-AI] {} model={} base_url={} request_kind={} elapsed_ms={} max_tokens={} input_chars={} error={}",
                        log_label,
                        self.model,
                        self.base_url,
                        request_kind,
                        started_at.elapsed().as_millis(),
                        requested_max_tokens_label,
                        input_chars,
                        err
                    );
                    return Err(err);
                }
            }
        };
        validate_summary_text(self, request_kind, started_at, text)
    }
}

fn transient_retry_backoff_ms(
    err: &str,
    retry_count: usize,
    max_transient_retries: usize,
) -> Option<u64> {
    if retry_count >= max_transient_retries || !is_transient_summary_error(err) {
        return None;
    }
    let next_retry = retry_count + 1;
    Some(200_u64 * next_retry as u64)
}

fn is_transient_summary_error(err: &str) -> bool {
    chatos_ai_runtime::is_transient_transport_or_parse_error(err)
}

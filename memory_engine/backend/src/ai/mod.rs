use std::time::Duration;

use reqwest::Client;
use serde_json::json;

use crate::config::AppConfig;
use crate::models::EngineModelProfile;

#[derive(Clone)]
pub struct AiClient {
    http: Client,
    api_key: Option<String>,
    base_url: String,
    model: String,
    temperature: f64,
    timeout_secs: u64,
    allow_rule_fallback: bool,
}

impl AiClient {
    pub fn new(config: &AppConfig) -> Result<Self, String> {
        Self::new_with_profile(config, None)
    }

    pub fn new_with_profile(
        config: &AppConfig,
        profile: Option<&EngineModelProfile>,
    ) -> Result<Self, String> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|err| err.to_string())?;
        let api_key = profile
            .and_then(|item| item.api_key.clone())
            .or_else(|| config.openai_api_key.clone());
        let base_url = profile
            .and_then(|item| item.base_url.clone())
            .unwrap_or_else(|| config.openai_base_url.clone());
        let model = profile
            .map(|item| item.model.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| config.openai_model.trim().to_string());
        let temperature = profile
            .and_then(|item| item.temperature)
            .unwrap_or(config.openai_temperature)
            .clamp(0.0, 2.0);
        Ok(Self {
            http,
            api_key,
            base_url: normalize_base_url(base_url.as_str()),
            model,
            temperature,
            timeout_secs: config.ai_request_timeout_secs,
            allow_rule_fallback: config.allow_rule_summary_fallback,
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    }

    pub fn allow_rule_fallback(&self) -> bool {
        self.allow_rule_fallback
    }

    pub async fn summarize(
        &self,
        title: Option<&str>,
        input: &str,
        max_tokens: Option<i64>,
    ) -> Result<String, String> {
        let api_key = self
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing MEMORY_ENGINE_OPENAI_API_KEY".to_string())?;

        let system_prompt = "You summarize conversation increments for a memory engine. Produce a concise, high-signal summary with concrete user intent, assistant response, and notable constraints. Do not use markdown bullets unless useful.";
        let user_prompt = match title.map(str::trim).filter(|value| !value.is_empty()) {
            Some(title) => format!("Thread title: {title}\n\nConversation increment:\n{input}"),
            None => format!("Conversation increment:\n{input}"),
        };

        let endpoint = format!("{}/chat/completions", self.base_url);
        let body = json!({
            "model": self.model,
            "temperature": self.temperature,
            "max_tokens": max_tokens.unwrap_or(500).clamp(128, 4000),
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        });

        let response = self
            .http
            .post(endpoint.as_str())
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(self.timeout_secs))
            .json(&body)
            .send()
            .await
            .map_err(|err| format!("ai request failed: {err}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("ai request status={} body={}", status, body));
        }

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|err| format!("ai response decode failed: {err}"))?;
        extract_chat_completion_text(&value).ok_or_else(|| "ai empty content".to_string())
    }
}

fn normalize_base_url(input: &str) -> String {
    input.trim().trim_end_matches('/').to_string()
}

fn extract_chat_completion_text(value: &serde_json::Value) -> Option<String> {
    value
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

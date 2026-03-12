use std::time::Duration;

use reqwest::Client;
use serde_json::json;

use crate::config::AppConfig;
use crate::models::AiModelConfig;

#[derive(Clone)]
pub struct AiClient {
    http: Client,
    default_api_key: Option<String>,
    default_base_url: String,
    default_model: String,
    default_temperature: f64,
    allow_local_fallback: bool,
}

impl AiClient {
    pub fn new(timeout_secs: u64, config: &AppConfig) -> Result<Self, String> {
        let http = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            default_api_key: config.openai_api_key.clone(),
            default_base_url: normalize_base_url(config.openai_base_url.as_str()),
            default_model: normalize_model_name(config.openai_model.as_str()),
            default_temperature: config.openai_temperature.clamp(0.0, 2.0),
            allow_local_fallback: config.allow_local_summary_fallback,
        })
    }

    pub async fn summarize(
        &self,
        model_cfg: Option<&AiModelConfig>,
        target_tokens: i64,
        prompt_title: &str,
        chunks: &[String],
    ) -> Result<String, String> {
        if chunks.is_empty() {
            return Err("empty summarize input".to_string());
        }

        let max_tokens = target_tokens.max(128).min(4000);

        let context = chunks
            .iter()
            .enumerate()
            .map(|(idx, c)| format!("[{}]\n{}", idx + 1, c))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let system_prompt = format!(
            "你是 Memory Server 的总结引擎。请输出结构化简洁总结，重点保留事实、决策、风险、待办。目标长度约 {} tokens。",
            max_tokens
        );
        let user_prompt = format!(
            "任务：{}\n\n请基于以下内容生成高质量总结：\n\n{}",
            prompt_title, context
        );

        if let Some(cfg) = model_cfg.filter(|cfg| cfg.enabled == 1) {
            let api_key = cfg
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .or(self.default_api_key.as_deref());

            let model_name = normalize_model_name(cfg.model.as_str());
            let base_url = cfg
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(normalize_base_url)
                .unwrap_or_else(|| self.default_base_url.clone());
            let temperature = cfg
                .temperature
                .unwrap_or(self.default_temperature)
                .clamp(0.0, 2.0);

            if let Some(api_key) = api_key {
                return self
                    .call_openai_compatible(
                        base_url.as_str(),
                        model_name.as_str(),
                        temperature,
                        api_key,
                        &system_prompt,
                        &user_prompt,
                        max_tokens,
                    )
                    .await;
            }
        }

        if let Some(api_key) = self.default_api_key.as_deref() {
            return self
                .call_openai_compatible(
                    self.default_base_url.as_str(),
                    self.default_model.as_str(),
                    self.default_temperature,
                    api_key,
                    &system_prompt,
                    &user_prompt,
                    max_tokens,
                )
                .await;
        }

        if self.allow_local_fallback {
            return Ok(local_fallback_summary(chunks, max_tokens as usize));
        }

        Err(
            "no available AI credentials: set model api_key in model config or MEMORY_SERVER_OPENAI_API_KEY"
                .to_string(),
        )
    }

    async fn call_openai_compatible(
        &self,
        base_url: &str,
        model: &str,
        temperature: f64,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: i64,
    ) -> Result<String, String> {
        let endpoint = build_chat_completion_endpoint(base_url);

        let body = json!({
            "model": model,
            "temperature": temperature.clamp(0.0, 2.0),
            "max_tokens": max_tokens,
            "messages": [
                {"role":"system","content": system_prompt},
                {"role":"user","content": user_prompt}
            ]
        });

        let resp = self
            .http
            .post(endpoint)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("ai request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("ai request status={} body={}", status, text));
        }

        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("ai response parse failed: {e}"))?;

        let text = value
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "ai empty content".to_string())?;

        Ok(text)
    }
}

fn normalize_base_url(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.is_empty() {
        "https://api.openai.com/v1".to_string()
    } else {
        normalized.to_string()
    }
}

fn normalize_model_name(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        "gpt-4o-mini".to_string()
    } else {
        trimmed.to_string()
    }
}

fn build_chat_completion_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/chat/completions") {
        normalized
    } else {
        format!("{}/chat/completions", normalized)
    }
}

fn local_fallback_summary(chunks: &[String], max_tokens: usize) -> String {
    let mut lines = vec![
        "[fallback-summary] 未配置可用模型，使用本地降级摘要。".to_string(),
        "关键要点：".to_string(),
    ];

    for chunk in chunks.iter().take(8) {
        let short = chunk
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(260)
            .collect::<String>();
        lines.push(format!("- {}", short));
    }

    let mut out = lines.join("\n");
    let approx_chars = max_tokens.saturating_mul(4);
    if out.len() > approx_chars {
        out = out.chars().take(approx_chars).collect();
    }
    out
}

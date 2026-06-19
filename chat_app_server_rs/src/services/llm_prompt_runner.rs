use serde_json::{Value, json};

use chatos_ai_runtime as shared_ai_runtime;

#[derive(Debug, Clone)]
pub struct PromptRunnerRuntime {
    pub config: shared_ai_runtime::ModelRuntimeConfig,
}

impl PromptRunnerRuntime {
    pub async fn from_ai_model_config(
        model_config_id: Option<String>,
        user_id: Option<String>,
        model_cfg: &Value,
        default_model: &str,
    ) -> Result<Self, String> {
        let config =
            crate::services::shared_ai_runtime::resolve_shared_model_runtime_config_for_request(
                model_config_id.as_deref(),
                Some(model_cfg),
                None,
                user_id.as_deref(),
                default_model,
                Some(false),
                true,
            )
            .await?;

        Ok(Self { config })
    }

    pub fn model(&self) -> &str {
        self.config.model.as_str()
    }

    pub fn provider(&self) -> &str {
        self.config.provider.as_str()
    }

    pub fn temperature(&self) -> f64 {
        self.config.temperature.unwrap_or(0.7)
    }

    pub fn api_key(&self) -> &str {
        self.config.api_key.as_str()
    }

    pub fn base_url(&self) -> &str {
        self.config.base_url.as_str()
    }
}

pub async fn run_text_prompt_with_runtime(
    runtime: &PromptRunnerRuntime,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    purpose: &str,
) -> Result<String, String> {
    if runtime.api_key().trim().is_empty() {
        return Err("未配置可用的 API Key".to_string());
    }
    if runtime.base_url().trim().is_empty() {
        return Err("未配置可用的 Base URL".to_string());
    }

    let content =
        run_with_responses(runtime, system_prompt, user_prompt, max_tokens, purpose).await?;

    let text = content.trim().to_string();
    if text.is_empty() {
        return Err("AI 未返回文本内容".to_string());
    }
    Ok(text)
}

pub async fn run_text_prompt_with_model_config(
    model_config_id: Option<String>,
    user_id: Option<String>,
    model_cfg: Option<Value>,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    default_model: &str,
    purpose: &str,
) -> Result<String, String> {
    let model_cfg = model_cfg.unwrap_or_else(|| json!({}));
    let runtime = PromptRunnerRuntime::from_ai_model_config(
        model_config_id,
        user_id,
        &model_cfg,
        default_model,
    )
    .await?;
    run_text_prompt_with_runtime(&runtime, system_prompt, user_prompt, max_tokens, purpose).await
}

async fn run_with_responses(
    runtime: &PromptRunnerRuntime,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    purpose: &str,
) -> Result<String, String> {
    let handler = shared_ai_runtime::AiRequestHandler::new();
    let response = shared_ai_runtime::run_compatible_prompt_with(
        &handler,
        &runtime.config,
        user_prompt,
        shared_ai_runtime::SimplePromptOptions {
            system_prompt: Some(system_prompt.to_string()),
            temperature: Some(runtime.temperature()),
            max_output_tokens: max_tokens,
            max_attempts: Some(if purpose == "session_summary_job" {
                5
            } else {
                4
            }),
            callbacks: shared_ai_runtime::StreamCallbacks::default(),
        },
        shared_ai_runtime::build_responses_text_input,
    )
    .await?;

    Ok(select_response_text(response.content, response.reasoning))
}

fn select_response_text(content: String, reasoning: Option<String>) -> String {
    shared_ai_runtime::select_preferred_response_text(content.as_str(), reasoning.as_deref())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::services::v2::ai_request_handler as v2_handler;
use crate::services::v2::message_manager as v2_message_manager;
use crate::services::v3::ai_request_handler as v3_handler;
use crate::services::v3::message_manager as v3_message_manager;

#[derive(Debug, Clone)]
pub struct PromptRunnerRuntime {
    pub model: String,
    pub provider: String,
    pub thinking_level: Option<String>,
    pub temperature: f64,
    pub api_key: String,
    pub base_url: String,
    pub supports_responses: bool,
}

impl PromptRunnerRuntime {
    pub fn from_ai_model_config(model_cfg: &Value, default_model: &str) -> Self {
        let cfg = Config::get();
        let resolved = resolve_chat_model_config(
            model_cfg,
            default_model,
            &cfg.openai_api_key,
            &cfg.openai_base_url,
            Some(false),
            true,
        );

        let supports_responses = model_cfg
            .get("supports_responses")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        Self {
            model: resolved.model,
            provider: resolved.provider,
            thinking_level: resolved.thinking_level,
            temperature: resolved.temperature,
            api_key: resolved.api_key,
            base_url: resolved.base_url,
            supports_responses,
        }
    }
}

pub async fn run_text_prompt_with_runtime(
    runtime: &PromptRunnerRuntime,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    purpose: &str,
) -> Result<String, String> {
    if runtime.api_key.trim().is_empty() {
        return Err("未配置可用的 API Key".to_string());
    }
    if runtime.base_url.trim().is_empty() {
        return Err("未配置可用的 Base URL".to_string());
    }

    let content = if runtime.supports_responses {
        run_with_responses(runtime, system_prompt, user_prompt, max_tokens, purpose).await?
    } else {
        run_with_chat_completions(runtime, system_prompt, user_prompt, max_tokens, purpose).await?
    };

    let text = content.trim().to_string();
    if text.is_empty() {
        return Err("AI 未返回文本内容".to_string());
    }
    Ok(text)
}

pub async fn run_text_prompt_with_model_config(
    model_cfg: Option<Value>,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    default_model: &str,
    purpose: &str,
) -> Result<String, String> {
    let model_cfg = model_cfg.unwrap_or_else(|| json!({}));
    let runtime = PromptRunnerRuntime::from_ai_model_config(&model_cfg, default_model);
    run_text_prompt_with_runtime(&runtime, system_prompt, user_prompt, max_tokens, purpose).await
}

async fn run_with_chat_completions(
    runtime: &PromptRunnerRuntime,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    purpose: &str,
) -> Result<String, String> {
    let handler = v2_handler::AiRequestHandler::new(
        runtime.api_key.clone(),
        runtime.base_url.clone(),
        v2_message_manager::MessageManager::new(),
    );

    let mut no_system_messages = base_url_disallows_system_messages(&runtime.base_url);
    for _ in 0..2 {
        let messages = build_chat_prompt_messages(system_prompt, user_prompt, no_system_messages);

        match handler
            .handle_request(
                messages,
                None,
                runtime.model.clone(),
                Some(runtime.temperature),
                max_tokens,
                v2_handler::StreamCallbacks {
                    on_chunk: None,
                    on_thinking: None,
                },
                false,
                Some(runtime.provider.clone()),
                runtime.thinking_level.clone(),
                None,
                true,
                None,
                None,
                purpose,
            )
            .await
        {
            Ok(response) => {
                return Ok(select_response_text(response.content, response.reasoning));
            }
            Err(err) => {
                if !no_system_messages && is_system_messages_not_allowed_error(&err) {
                    no_system_messages = true;
                    continue;
                }
                return Err(err);
            }
        }
    }

    Err("AI 请求失败：系统消息兼容重试后仍失败".to_string())
}

async fn run_with_responses(
    runtime: &PromptRunnerRuntime,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<i64>,
    purpose: &str,
) -> Result<String, String> {
    let handler = v3_handler::AiRequestHandler::new(
        runtime.api_key.clone(),
        runtime.base_url.clone(),
        v3_message_manager::MessageManager::new(),
    );

    let mut no_system_messages = base_url_disallows_system_messages(&runtime.base_url);
    let mut input_as_list = false;
    for _ in 0..4 {
        let wrapped_user_prompt = if no_system_messages && !system_prompt.trim().is_empty() {
            format!(
                "【系统上下文】\n{}\n\n{}",
                system_prompt.trim(),
                user_prompt
            )
        } else {
            user_prompt.to_string()
        };
        let instructions = if no_system_messages {
            None
        } else {
            Some(system_prompt.to_string())
        };
        let input = build_responses_input(wrapped_user_prompt.as_str(), input_as_list);

        match handler
            .handle_request(
                input,
                runtime.model.clone(),
                instructions,
                None,
                None,
                Some(runtime.temperature),
                max_tokens,
                v3_handler::StreamCallbacks {
                    on_chunk: None,
                    on_thinking: None,
                },
                Some(runtime.provider.clone()),
                runtime.thinking_level.clone(),
                None,
                true,
                None,
                None,
                purpose,
            )
            .await
        {
            Ok(response) => {
                return Ok(select_response_text(response.content, response.reasoning));
            }
            Err(err) => {
                if !input_as_list && is_input_must_be_list_error(&err) {
                    input_as_list = true;
                    continue;
                }
                if !no_system_messages && is_system_messages_not_allowed_error(&err) {
                    no_system_messages = true;
                    continue;
                }
                return Err(err);
            }
        }
    }

    Err("AI 请求失败：responses 兼容重试后仍失败".to_string())
}

fn build_chat_prompt_messages(
    system_prompt: &str,
    user_prompt: &str,
    no_system_messages: bool,
) -> Vec<Value> {
    if !no_system_messages {
        return vec![
            json!({"role": "system", "content": system_prompt}),
            json!({"role": "user", "content": user_prompt}),
        ];
    }

    let mut messages = Vec::new();
    let normalized_system = system_prompt.trim();
    if !normalized_system.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": format!("【系统上下文】\n{}", normalized_system)
        }));
    }
    messages.push(json!({"role": "user", "content": user_prompt}));
    messages
}

fn base_url_disallows_system_messages(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    if let Ok(value) = std::env::var("DISABLE_SYSTEM_MESSAGES_FOR_PROXY") {
        let normalized = value.trim().to_lowercase();
        return normalized == "1"
            || normalized == "true"
            || normalized == "yes"
            || normalized == "on";
    }

    false
}

fn is_system_messages_not_allowed_error(err: &str) -> bool {
    err.to_lowercase()
        .contains("system messages are not allowed")
}

fn build_responses_input(user_prompt: &str, input_as_list: bool) -> Value {
    if !input_as_list {
        return Value::String(user_prompt.to_string());
    }

    json!([
        {
            "role": "user",
            "content": user_prompt
        }
    ])
}

fn is_input_must_be_list_error(err: &str) -> bool {
    let normalized = err.to_lowercase();
    normalized.contains("input must be a list")
}

fn select_response_text(content: String, reasoning: Option<String>) -> String {
    if !content.trim().is_empty() {
        return content;
    }

    if let Some(reasoning) = reasoning {
        if !reasoning.trim().is_empty() {
            return reasoning;
        }
    }

    String::new()
}

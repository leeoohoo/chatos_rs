use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::task;

use crate::config::Config;
use crate::core::chat_stream::{build_v2_callbacks, send_fallback_chunk_if_needed};
use crate::models::session::SessionService;
use crate::repositories::system_contexts;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v2::agent::{load_model_config_for_agent, run_chat};
use crate::services::v2::ai_server::{AiServer, ChatOptions};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::events::Events;
use crate::utils::log_helpers::log_chat_error;
use crate::utils::model_config::{normalize_provider, normalize_thinking_level};
use crate::utils::sse::{sse_channel, SseSender};

#[derive(Debug, Deserialize)]
struct ChatRequest {
    session_id: Option<String>,
    content: Option<String>,
    ai_model_config: Option<Value>,
    user_id: Option<String>,
    reasoning_enabled: Option<bool>,
    agent_id: Option<String>,
    attachments: Option<Vec<Value>>,
}

pub fn router() -> Router {
    Router::new().route("/chat/stream", post(chat_stream))
}

async fn chat_stream(
    Json(req): Json<ChatRequest>,
) -> Result<
    axum::response::Sse<
        impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    (StatusCode, Json<Value>),
> {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "session_id 和 content 不能为空"})),
        ));
    }
    abort_registry::reset(&session_id);
    let (sse, sender) = sse_channel();
    if req.agent_id.is_some() {
        task::spawn(stream_chat_v2_agent(sender, req));
    } else {
        task::spawn(stream_chat_v2(sender, req));
    }
    Ok(sse)
}

async fn stream_chat_v2_agent(sender: SseSender, req: ChatRequest) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let agent_id = req.agent_id.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() || agent_id.is_empty() {
        sender.send_json(&json!({ "type": Events::ERROR, "timestamp": crate::core::time::now_rfc3339(), "data": { "error": "session_id, content 和 agent_id 为必填项" } }));
        sender.send_done();
        return;
    }

    sender.send_json(&json!({ "type": Events::START, "timestamp": crate::core::time::now_rfc3339(), "session_id": session_id }));

    let model_cfg = match load_model_config_for_agent(&agent_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            log_chat_error(&err);
            sender.send_json(&json!({ "type": Events::ERROR, "timestamp": crate::core::time::now_rfc3339(), "data": { "error": err } }));
            sender.send_done();
            return;
        }
    };

    let callback_bundle = build_v2_callbacks(&sender, &session_id);
    let chunk_sent = callback_bundle.chunk_sent;

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);

    let result = run_chat(
        &session_id,
        &content,
        &model_cfg,
        req.user_id.clone(),
        att,
        req.reasoning_enabled,
        callback_bundle.callbacks,
    )
    .await;

    match result {
        Ok(res) => {
            if abort_registry::is_aborted(&session_id) {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": crate::core::time::now_rfc3339() }));
            } else {
                send_fallback_chunk_if_needed(&sender, &chunk_sent, &res);
                sender.send_json(&json!({ "type": Events::COMPLETE, "timestamp": crate::core::time::now_rfc3339(), "result": res }));
            }
        }
        Err(err) => {
            if abort_registry::is_aborted(&session_id) {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": crate::core::time::now_rfc3339() }));
            } else {
                log_chat_error(&err);
                sender.send_json(&json!({ "type": Events::ERROR, "timestamp": crate::core::time::now_rfc3339(), "data": { "error": err } }));
            }
        }
    }
    sender.send_done();
}

async fn stream_chat_v2(sender: SseSender, req: ChatRequest) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() {
        sender.send_json(&json!({ "type": Events::ERROR, "timestamp": crate::core::time::now_rfc3339(), "data": { "error": "session_id 和 content 不能为空" } }));
        sender.send_done();
        return;
    }

    sender.send_json(&json!({ "type": Events::START, "timestamp": crate::core::time::now_rfc3339(), "session_id": session_id }));

    let cfg = Config::get();
    let model_cfg = req.ai_model_config.unwrap_or_else(|| json!({}));
    let model = model_cfg
        .get("model_name")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4")
        .to_string();
    let provider = normalize_provider(
        model_cfg
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("gpt"),
    );
    let thinking_level = normalize_thinking_level(
        &provider,
        model_cfg.get("thinking_level").and_then(|v| v.as_str()),
    )
    .ok()
    .flatten();
    let temperature = model_cfg
        .get("temperature")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.7);
    let supports_images = model_cfg
        .get("supports_images")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let supports_reasoning = model_cfg
        .get("supports_reasoning")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let reasoning_enabled = req.reasoning_enabled.unwrap_or_else(|| {
        model_cfg
            .get("reasoning_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    });
    let effective_reasoning = (supports_reasoning || thinking_level.is_some()) && reasoning_enabled;
    let use_tools = false;
    let api_key = model_cfg
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or(&cfg.openai_api_key)
        .to_string();
    let base_url = model_cfg
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or(&cfg.openai_base_url)
        .to_string();

    let (http_servers, stdio_servers, builtin_servers) = (Vec::new(), Vec::new(), Vec::new());
    let mut mcp_exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    let _ = mcp_exec.init().await;

    let mut ai_server = AiServer::new(
        api_key.clone(),
        base_url.clone(),
        model.clone(),
        temperature,
        mcp_exec,
    );

    let mut effective_user_id = req.user_id.clone();
    if effective_user_id.is_none() && !session_id.is_empty() {
        if let Ok(Some(sess)) = SessionService::get_by_id(&session_id).await {
            effective_user_id = sess.user_id;
        }
    }

    let allow_active_ctx = model_cfg
        .get("use_active_system_context")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if let Some(prompt) = model_cfg
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        ai_server.set_system_prompt(Some(prompt));
    } else if allow_active_ctx {
        if let Some(uid) = effective_user_id.clone() {
            if let Ok(Some(ctx)) = system_contexts::get_active_system_context(&uid).await {
                if let Some(content) = ctx.content {
                    ai_server.set_system_prompt(Some(content));
                }
            }
        }
    }

    let effective_settings = get_effective_user_settings(effective_user_id.clone())
        .await
        .unwrap_or_else(|_| json!({}));
    apply_settings_to_ai_client(&mut ai_server.ai_client, &effective_settings);
    let max_tokens = effective_settings
        .get("CHAT_MAX_TOKENS")
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0);

    let callback_bundle = build_v2_callbacks(&sender, &session_id);
    let chunk_sent = callback_bundle.chunk_sent;

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);

    let result = ai_server
        .chat(
            &session_id,
            &content,
            ChatOptions {
                model: Some(model.clone()),
                provider: Some(provider),
                thinking_level,
                temperature: Some(temperature),
                max_tokens,
                use_tools: Some(use_tools),
                attachments: Some(att),
                supports_images: Some(supports_images),
                reasoning_enabled: Some(effective_reasoning),
                callbacks: Some(callback_bundle.callbacks),
            },
        )
        .await;

    match result {
        Ok(res) => {
            if abort_registry::is_aborted(&session_id) {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": crate::core::time::now_rfc3339() }));
            } else {
                send_fallback_chunk_if_needed(&sender, &chunk_sent, &res);
                sender.send_json(&json!({ "type": Events::COMPLETE, "timestamp": crate::core::time::now_rfc3339(), "result": res }));
            }
        }
        Err(err) => {
            if abort_registry::is_aborted(&session_id) {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": crate::core::time::now_rfc3339() }));
            } else {
                log_chat_error(&err);
                sender.send_json(&json!({ "type": Events::ERROR, "timestamp": crate::core::time::now_rfc3339(), "data": { "error": err } }));
            }
        }
    }
    sender.send_done();
}

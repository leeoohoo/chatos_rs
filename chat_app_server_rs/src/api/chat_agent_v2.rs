use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::task;

use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::ai_settings::chat_max_tokens_from_settings;
use crate::core::chat_stream::{
    build_v2_callbacks, send_cancelled_event, send_complete_event, send_error_event,
    send_fallback_chunk_if_needed, send_start_event,
};
use crate::models::session::SessionService;
use crate::repositories::system_contexts;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v2::agent::{load_model_config_for_agent, run_chat};
use crate::services::v2::ai_server::{AiServer, ChatOptions};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::log_helpers::log_chat_error;
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
        send_error_event(&sender, "session_id, content 和 agent_id 为必填项");
        sender.send_done();
        return;
    }

    send_start_event(&sender, &session_id);

    let model_cfg = match load_model_config_for_agent(&agent_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            log_chat_error(&err);
            send_error_event(&sender, &err);
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
                send_cancelled_event(&sender);
            } else {
                send_fallback_chunk_if_needed(&sender, &chunk_sent, &res);
                send_complete_event(&sender, &res);
            }
        }
        Err(err) => {
            if abort_registry::is_aborted(&session_id) {
                send_cancelled_event(&sender);
            } else {
                log_chat_error(&err);
                send_error_event(&sender, &err);
            }
        }
    }
    sender.send_done();
}

async fn stream_chat_v2(sender: SseSender, req: ChatRequest) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() {
        send_error_event(&sender, "session_id 和 content 不能为空");
        sender.send_done();
        return;
    }

    send_start_event(&sender, &session_id);

    let cfg = Config::get();
    let model_cfg = req.ai_model_config.unwrap_or_else(|| json!({}));
    let model_runtime = resolve_chat_model_config(
        &model_cfg,
        "gpt-4",
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        req.reasoning_enabled,
        true,
    );
    let use_tools = false;

    let (http_servers, stdio_servers, builtin_servers) = (Vec::new(), Vec::new(), Vec::new());
    let mut mcp_exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    let _ = mcp_exec.init().await;

    let mut ai_server = AiServer::new(
        model_runtime.api_key.clone(),
        model_runtime.base_url.clone(),
        model_runtime.model.clone(),
        model_runtime.temperature,
        mcp_exec,
    );

    let mut effective_user_id = req.user_id.clone();
    if effective_user_id.is_none() && !session_id.is_empty() {
        if let Ok(Some(sess)) = SessionService::get_by_id(&session_id).await {
            effective_user_id = sess.user_id;
        }
    }

    if let Some(prompt) = model_runtime.system_prompt.clone() {
        ai_server.set_system_prompt(Some(prompt));
    } else if model_runtime.use_active_system_context {
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
    let max_tokens = chat_max_tokens_from_settings(&effective_settings);

    let callback_bundle = build_v2_callbacks(&sender, &session_id);
    let chunk_sent = callback_bundle.chunk_sent;

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);

    let result = ai_server
        .chat(
            &session_id,
            &content,
            ChatOptions {
                model: Some(model_runtime.model.clone()),
                provider: Some(model_runtime.provider.clone()),
                thinking_level: model_runtime.thinking_level.clone(),
                temperature: Some(model_runtime.temperature),
                max_tokens,
                use_tools: Some(use_tools),
                attachments: Some(att),
                supports_images: Some(model_runtime.supports_images),
                reasoning_enabled: Some(model_runtime.effective_reasoning),
                callbacks: Some(callback_bundle.callbacks),
            },
        )
        .await;

    match result {
        Ok(res) => {
            if abort_registry::is_aborted(&session_id) {
                send_cancelled_event(&sender);
            } else {
                send_fallback_chunk_if_needed(&sender, &chunk_sent, &res);
                send_complete_event(&sender, &res);
            }
        }
        Err(err) => {
            if abort_registry::is_aborted(&session_id) {
                send_cancelled_event(&sender);
            } else {
                log_chat_error(&err);
                send_error_event(&sender, &err);
            }
        }
    }
    sender.send_done();
}

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::task;

use crate::config::Config;
use crate::models::message::MessageService;
use crate::models::session::SessionService;
use crate::repositories::system_contexts;
use crate::services::mcp_loader::load_mcp_configs_for_user;
use crate::services::session_title::maybe_rename_session_title;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v3::ai_client::AiClientCallbacks;
use crate::services::v3::ai_server::{AiServer, ChatOptions};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::events::Events;
use crate::utils::log_helpers::{log_chat_begin, log_chat_cancelled, log_chat_error};
use crate::utils::model_config::{normalize_provider, normalize_thinking_level};
use crate::utils::sse::{sse_channel, SseSender};

#[derive(Debug, Deserialize)]
struct ChatRequest {
    session_id: Option<String>,
    content: Option<String>,
    ai_model_config: Option<Value>,
    user_id: Option<String>,
    attachments: Option<Vec<Value>>,
    reasoning_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/agent_v3/chat/stream", post(agent_chat_stream))
        .route("/api/agent_v3/chat/stop", post(stop_chat))
        .route("/api/agent_v3/tools", get(agent_tools))
        .route("/api/agent_v3/status", get(agent_status))
        .route(
            "/api/agent_v3/session/:session_id/reset",
            post(reset_session),
        )
}

async fn agent_chat_stream(
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
    if req
        .ai_model_config
        .as_ref()
        .and_then(|cfg| cfg.get("supports_responses").and_then(|v| v.as_bool()))
        != Some(true)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "当前模型未启用 Responses API"})),
        ));
    }

    abort_registry::reset(&session_id);
    let (sse, sender) = sse_channel();
    task::spawn(stream_chat_v3(sender, req));
    Ok(sse)
}

async fn agent_tools(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let (http_servers, stdio_servers, builtin_servers) =
        load_mcp_configs_for_user(query.user_id, None, None, None)
            .await
            .unwrap_or((Vec::new(), Vec::new(), Vec::new()));
    let mut exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    if let Err(err) = exec.init().await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        );
    }
    let tools = exec.get_tools();
    (
        StatusCode::OK,
        Json(json!({
            "tools": tools,
            "count": tools.len(),
            "servers": { "http": http_servers.len(), "stdio": stdio_servers.len(), "builtin": builtin_servers.len() }
        })),
    )
}

async fn agent_status() -> Json<Value> {
    let cfg = Config::get();
    Json(json!({
        "status": "ok",
        "version": "3.0.0",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "openai": {
            "configured": !cfg.openai_api_key.is_empty(),
            "base_url": cfg.openai_base_url.clone()
        }
    }))
}

async fn reset_session(Path(session_id): Path<String>) -> Json<Value> {
    let _ = MessageService::delete_by_session(&session_id).await;
    Json(json!({"success": true, "message": "会话重置成功", "session_id": session_id}))
}

async fn stop_chat(Json(req): Json<Value>) -> (StatusCode, Json<Value>) {
    let session_id = req.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    if session_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "message": "缺少 session_id"})),
        );
    }
    let ok = abort_registry::abort(session_id);
    if ok {
        return (
            StatusCode::OK,
            Json(json!({"success": true, "message": "停止中"})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({"success": false, "message": "未找到可停止的会话或已停止"})),
    )
}

async fn stream_chat_v3(sender: SseSender, req: ChatRequest) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let cfg = Config::get();

    sender.send_json(&json!({ "type": Events::START, "timestamp": chrono::Utc::now().to_rfc3339(), "session_id": session_id }));

    if !session_id.is_empty() && !content.is_empty() {
        let sid = session_id.clone();
        let text = content.clone();
        tokio::spawn(async move {
            let _ = maybe_rename_session_title(&sid, &text, 30).await;
        });
    }

    let model_cfg = req.ai_model_config.unwrap_or_else(|| json!({}));
    if model_cfg
        .get("supports_responses")
        .and_then(|v| v.as_bool())
        != Some(true)
    {
        sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": "当前模型未启用 Responses API" } }));
        return;
    }

    let model = model_cfg
        .get("model_name")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4o")
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

    if let Some(prompt) = model_cfg
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        ai_server.set_system_prompt(Some(prompt));
    } else if let Some(uid) = effective_user_id.clone() {
        if let Ok(Some(ctx)) = system_contexts::get_active_system_context(&uid).await {
            if let Some(content) = ctx.content {
                ai_server.set_system_prompt(Some(content));
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

    log_chat_begin(
        &session_id,
        &model,
        &base_url,
        use_tools,
        http_servers.len(),
        stdio_servers.len() + builtin_servers.len(),
        !api_key.is_empty(),
    );

    let chunk_sent = std::sync::Arc::new(AtomicBool::new(false));
    let sender_clone = sender.clone();
    let sid_clone = session_id.clone();
    let chunk_flag = chunk_sent.clone();
    let on_chunk = move |chunk: String| {
        if abort_registry::is_aborted(&sid_clone) {
            return;
        }
        chunk_flag.store(true, Ordering::Relaxed);
        sender_clone.send_json(&json!({ "type": Events::CHUNK, "timestamp": chrono::Utc::now().to_rfc3339(), "content": chunk }));
    };
    let sender_thinking = sender.clone();
    let sid_thinking = session_id.clone();
    let on_thinking = move |chunk: String| {
        if abort_registry::is_aborted(&sid_thinking) {
            return;
        }
        sender_thinking.send_json(&json!({ "type": Events::THINKING, "timestamp": chrono::Utc::now().to_rfc3339(), "content": chunk }));
    };
    let sender_tools = sender.clone();
    let sid_tools = session_id.clone();
    let on_tools_start = move |tool_calls: Value| {
        if abort_registry::is_aborted(&sid_tools) {
            return;
        }
        sender_tools.send_json(&json!({ "type": Events::TOOLS_START, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "tool_calls": tool_calls } }));
    };
    let sender_tools_stream = sender.clone();
    let sid_tools_stream = session_id.clone();
    let on_tools_stream = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_stream) {
            return;
        }
        sender_tools_stream.send_json(&json!({ "type": Events::TOOLS_STREAM, "timestamp": chrono::Utc::now().to_rfc3339(), "data": result }));
    };
    let sender_tools_end = sender.clone();
    let sid_tools_end = session_id.clone();
    let on_tools_end = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_end) {
            return;
        }
        sender_tools_end.send_json(&json!({ "type": Events::TOOLS_END, "timestamp": chrono::Utc::now().to_rfc3339(), "data": result }));
    };

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);

    let callbacks = AiClientCallbacks {
        on_chunk: Some(std::sync::Arc::new(on_chunk)),
        on_thinking: Some(std::sync::Arc::new(on_thinking)),
        on_tools_start: Some(std::sync::Arc::new(on_tools_start)),
        on_tools_stream: Some(std::sync::Arc::new(on_tools_stream)),
        on_tools_end: Some(std::sync::Arc::new(on_tools_end)),
    };

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
                callbacks: Some(callbacks),
            },
        )
        .await;

    let mut should_send_done = false;
    match result {
        Ok(res) => {
            if !abort_registry::is_aborted(&session_id) {
                if !chunk_sent.load(Ordering::Relaxed) {
                    if let Some(text) = res.get("content").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            sender.send_json(&json!({ "type": Events::CHUNK, "timestamp": chrono::Utc::now().to_rfc3339(), "content": text }));
                        }
                    }
                }
                sender.send_json(&json!({ "type": Events::COMPLETE, "timestamp": chrono::Utc::now().to_rfc3339(), "result": res }));
                should_send_done = true;
            } else {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": chrono::Utc::now().to_rfc3339() }));
            }
        }
        Err(err) => {
            if abort_registry::is_aborted(&session_id) {
                log_chat_cancelled(&session_id);
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": chrono::Utc::now().to_rfc3339() }));
            } else {
                log_chat_error(&err);
                sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": err } }));
            }
        }
    }
    if should_send_done {
        sender.send_done();
    }
}

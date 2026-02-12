use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::task;

use crate::config::Config;
use crate::core::chat_stream::{build_v2_callbacks, send_fallback_chunk_if_needed};
use crate::models::message::MessageService;
use crate::models::session::SessionService;
use crate::repositories::system_contexts;
use crate::services::mcp_loader::load_mcp_configs_for_user;
use crate::services::session_title::maybe_rename_session_title;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v2::ai_server::{AiServer, ChatOptions};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
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
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/agent_v2/chat/stream", post(agent_chat_stream))
        .route(
            "/api/agent_v2/chat/stream/simple",
            post(agent_chat_stream_simple),
        )
        .route("/api/agent_v2/tools", get(agent_tools))
        .route("/api/agent_v2/status", get(agent_status))
        .route(
            "/api/agent_v2/session/:session_id/reset",
            post(reset_session),
        )
        .route(
            "/api/agent_v2/session/:session_id/config",
            get(get_session_config).post(update_session_config),
        )
        .route("/api/chat/stop", post(stop_chat))
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

    abort_registry::reset(&session_id);
    let (sse, sender) = sse_channel();

    task::spawn(stream_chat_v2(sender, req, false, true, false));

    Ok(sse)
}

async fn agent_chat_stream_simple(
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
        task::spawn(stream_chat_v2_agent(sender, req, false));
    } else {
        task::spawn(stream_chat_v2(sender, req, true, false, true));
    }
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
    let tools = exec.get_available_tools();
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
        "version": "2.0.0",
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

async fn get_session_config(Path(session_id): Path<String>) -> Json<Value> {
    Json(
        json!({"success": true, "config": { "model": "gpt-4", "temperature": 0.7, "session_id": session_id }}),
    )
}

async fn update_session_config(
    Path(_session_id): Path<String>,
    Json(_req): Json<Value>,
) -> Json<Value> {
    Json(json!({"success": true, "message": "会话配置更新成功"}))
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

async fn stream_chat_v2(
    sender: SseSender,
    req: ChatRequest,
    always_send_done: bool,
    rename_session: bool,
    respect_model_flags: bool,
) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let cfg = Config::get();

    let start_evt = json!({ "type": Events::START, "timestamp": chrono::Utc::now().to_rfc3339(), "session_id": session_id });
    sender.send_json(&start_evt);

    if rename_session && !session_id.is_empty() && !content.is_empty() {
        let sid = session_id.clone();
        let text = content.clone();
        tokio::spawn(async move {
            let _ = maybe_rename_session_title(&sid, &text, 30).await;
        });
    }

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

    let allow_active_ctx = if respect_model_flags {
        model_cfg
            .get("use_active_system_context")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    } else {
        true
    };
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

    log_chat_begin(
        &session_id,
        &model,
        &base_url,
        use_tools,
        http_servers.len(),
        stdio_servers.len() + builtin_servers.len(),
        !api_key.is_empty(),
    );

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

    let mut should_send_done = false;
    match result {
        Ok(res) => {
            if !abort_registry::is_aborted(&session_id) {
                send_fallback_chunk_if_needed(&sender, &chunk_sent, &res);
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
    if always_send_done || should_send_done {
        sender.send_done();
    }
}

async fn stream_chat_v2_agent(sender: SseSender, req: ChatRequest, rename_session: bool) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let agent_id = req.agent_id.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() || agent_id.is_empty() {
        sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": "session_id, content 和 agent_id 为必填项" } }));
        sender.send_done();
        return;
    }

    let start_evt = json!({ "type": Events::START, "timestamp": chrono::Utc::now().to_rfc3339(), "session_id": session_id });
    sender.send_json(&start_evt);

    if rename_session && !session_id.is_empty() && !content.is_empty() {
        let sid = session_id.clone();
        let text = content.clone();
        tokio::spawn(async move {
            let _ = maybe_rename_session_title(&sid, &text, 30).await;
        });
    }

    let model_config = match crate::services::v2::agent::load_model_config_for_agent(&agent_id)
        .await
    {
        Ok(cfg) => cfg,
        Err(err) => {
            sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": err } }));
            sender.send_done();
            return;
        }
    };

    let callback_bundle = build_v2_callbacks(&sender, &session_id);
    let chunk_sent = callback_bundle.chunk_sent;

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);
    let result = crate::services::v2::agent::run_chat(
        &session_id,
        &content,
        &model_config,
        req.user_id.clone(),
        att,
        req.reasoning_enabled,
        callback_bundle.callbacks,
    )
    .await;

    match result {
        Ok(res) => {
            if !abort_registry::is_aborted(&session_id) {
                send_fallback_chunk_if_needed(&sender, &chunk_sent, &res);
                sender.send_json(&json!({ "type": Events::COMPLETE, "timestamp": chrono::Utc::now().to_rfc3339(), "result": res }));
            } else {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": chrono::Utc::now().to_rfc3339() }));
            }
        }
        Err(err) => {
            if abort_registry::is_aborted(&session_id) {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": chrono::Utc::now().to_rfc3339() }));
            } else {
                sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": err } }));
            }
        }
    }
    sender.send_done();
}

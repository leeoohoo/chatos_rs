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
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::ai_settings::chat_max_tokens_from_settings;
use crate::core::chat_context::{
    maybe_spawn_session_title_rename, resolve_effective_user_id, resolve_system_prompt,
};
use crate::core::chat_stream::{
    build_v2_callbacks, handle_chat_result, send_error_event, send_start_event,
};
use crate::core::mcp_runtime::load_mcp_servers_by_selection;
use crate::models::message::MessageService;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v2::ai_server::{AiServer, ChatOptions};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::log_helpers::{log_chat_begin, log_chat_cancelled, log_chat_error};
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
        load_mcp_servers_by_selection(query.user_id, false, Vec::new(), None, None).await;
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
        "timestamp": crate::core::time::now_rfc3339(),
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

    send_start_event(&sender, &session_id);

    maybe_spawn_session_title_rename(rename_session, &session_id, &content, 30);

    let model_cfg = req.ai_model_config.unwrap_or_else(|| json!({}));
    let model_runtime = resolve_chat_model_config(
        &model_cfg,
        "gpt-4",
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        req.reasoning_enabled,
        respect_model_flags,
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

    let effective_user_id = resolve_effective_user_id(req.user_id.clone(), &session_id).await;

    if let Some(prompt) = resolve_system_prompt(
        model_runtime.system_prompt.clone(),
        model_runtime.use_active_system_context,
        effective_user_id.clone(),
    )
    .await
    {
        ai_server.set_system_prompt(Some(prompt));
    }

    let effective_settings = get_effective_user_settings(effective_user_id.clone())
        .await
        .unwrap_or_else(|_| json!({}));
    apply_settings_to_ai_client(&mut ai_server.ai_client, &effective_settings);
    let max_tokens = chat_max_tokens_from_settings(&effective_settings);

    log_chat_begin(
        &session_id,
        &model_runtime.model,
        &model_runtime.base_url,
        use_tools,
        http_servers.len(),
        stdio_servers.len() + builtin_servers.len(),
        !model_runtime.api_key.is_empty(),
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

    let should_send_done = handle_chat_result(
        &sender,
        &session_id,
        Some(&chunk_sent),
        result,
        || log_chat_cancelled(&session_id),
        |err| log_chat_error(err),
    );
    if always_send_done || should_send_done {
        sender.send_done();
    }
}

async fn stream_chat_v2_agent(sender: SseSender, req: ChatRequest, rename_session: bool) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let agent_id = req.agent_id.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() || agent_id.is_empty() {
        send_error_event(&sender, "session_id, content 和 agent_id 为必填项");
        sender.send_done();
        return;
    }

    send_start_event(&sender, &session_id);

    maybe_spawn_session_title_rename(rename_session, &session_id, &content, 30);

    let model_config =
        match crate::services::v2::agent::load_model_config_for_agent(&agent_id).await {
            Ok(cfg) => cfg,
            Err(err) => {
                send_error_event(&sender, &err);
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

    let _ = handle_chat_result(
        &sender,
        &session_id,
        Some(&chunk_sent),
        result,
        || {},
        |_| {},
    );
    sender.send_done();
}

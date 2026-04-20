use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::chat_stream_common::{
    build_prefixed_input_items, resolve_chat_stream_context, sync_chat_turn_snapshot,
    validate_chat_stream_request, wire_implicit_command_tracking, ChatStreamRequest,
};
use crate::api::conversation_semantics::extract_conversation_scope_id;
use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::ai_settings::chat_max_tokens_from_settings;
use crate::core::auth::AuthUser;
use crate::core::chat_context::maybe_spawn_session_title_rename;
use crate::core::chat_runtime::project_id_from_metadata;
use crate::core::chat_stream::{
    build_v3_callbacks, handle_chat_result, send_error_event, send_start_event,
    send_tools_unavailable_event,
};
use crate::core::mcp_runtime::{load_mcp_servers_by_selection, McpServerBundle};
use crate::core::user_scope::{ensure_and_set_user_id, resolve_user_id};
use crate::services::ai_common::normalize_turn_id;
use crate::services::memory_server_client;
use crate::services::runtime_guidance_manager::{runtime_guidance_manager, EnqueueGuidanceError};
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v3::ai_server::{AiServer, ChatOptions};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::log_helpers::{log_chat_begin, log_chat_cancelled, log_chat_error};
use crate::utils::sse::{sse_channel, SseSender};
use tracing::warn;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuntimeGuidanceRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: Option<String>,
    turn_id: Option<String>,
    content: Option<String>,
    project_id: Option<String>,
}

fn normalize_project_scope_id(value: Option<&str>) -> String {
    let trimmed = value.unwrap_or_default().trim();
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/api/agent_v3/chat/stream", post(agent_chat_stream))
        .route("/api/agent_v3/chat/stop", post(stop_chat))
        .route("/api/agent_v3/chat/guide", post(submit_runtime_guidance))
        .route("/api/agent_v3/tools", get(agent_tools))
        .route("/api/agent_v3/status", get(agent_status))
        .route(
            "/api/agent_v3/conversation/:conversation_id/reset",
            post(reset_conversation),
        )
}

async fn agent_chat_stream(
    auth: AuthUser,
    Json(mut req): Json<ChatStreamRequest>,
) -> Result<
    axum::response::Sse<
        impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    (StatusCode, Json<Value>),
> {
    if let Err(err) = ensure_and_set_user_id(&mut req.user_id, &auth) {
        return Err(err);
    }
    validate_chat_stream_request(&req, true)?;
    let conversation_id = req.conversation_id.clone().unwrap_or_default();

    abort_registry::reset(&conversation_id);
    let (sse, sender) = sse_channel();
    memory_server_client::spawn_with_current_access_token(stream_chat_v3(sender, req));
    Ok(sse)
}

async fn agent_tools(auth: AuthUser, Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let (http_servers, stdio_servers, builtin_servers): McpServerBundle =
        load_mcp_servers_by_selection(Some(user_id), false, Vec::new(), None, None).await;
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
    let unavailable_tools = exec.get_unavailable_tools();
    (
        StatusCode::OK,
        Json(json!({
            "tools": tools,
            "count": tools.len(),
            "unavailable_tools": unavailable_tools,
            "unavailable_count": unavailable_tools.len(),
            "servers": { "http": http_servers.len(), "stdio": stdio_servers.len(), "builtin": builtin_servers.len() }
        })),
    )
}

async fn agent_status() -> Json<Value> {
    let cfg = Config::get();
    Json(json!({
        "status": "ok",
        "version": "3.0.0",
        "timestamp": crate::core::time::now_rfc3339(),
        "openai": {
            "configured": !cfg.openai_api_key.is_empty(),
            "base_url": cfg.openai_base_url.clone()
        }
    }))
}

async fn reset_conversation(Path(conversation_id): Path<String>) -> Json<Value> {
    let _ = memory_server_client::delete_messages_by_session(&conversation_id).await;
    Json(json!({
        "success": true,
        "message": "对话线程重置成功",
        "conversation_id": conversation_id
    }))
}

async fn stop_chat(Json(req): Json<Value>) -> (StatusCode, Json<Value>) {
    let conversation_id = extract_conversation_scope_id(&req).unwrap_or_default();
    if conversation_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "message": "缺少 conversation_id"})),
        );
    }
    let ok = abort_registry::abort(conversation_id.as_str());
    if ok {
        return (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "停止中",
                "conversation_id": conversation_id
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "success": false,
            "message": "未找到可停止的对话线程或已停止",
            "conversation_id": conversation_id
        })),
    )
}

async fn submit_runtime_guidance(
    auth: AuthUser,
    Json(req): Json<RuntimeGuidanceRequest>,
) -> (StatusCode, Json<Value>) {
    const CONTENT_MAX_LEN: usize = 1000;

    let session_id = req
        .conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let turn_id = normalize_turn_id(req.turn_id.as_deref()).unwrap_or_default();
    let content = req
        .content
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let requested_project_id = req
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if session_id.is_empty() || turn_id.is_empty() || content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "conversation_id / turn_id / content 不能为空",
                "code": "invalid_runtime_guidance_payload",
            })),
        );
    }
    if content.chars().count() > CONTENT_MAX_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": format!("content 长度不能超过 {} 字符", CONTENT_MAX_LEN),
                "code": "runtime_guidance_too_long",
                "max_length": CONTENT_MAX_LEN,
            })),
        );
    }

    let auth_user_id = match resolve_user_id(None, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let target_session = match memory_server_client::get_session_by_id(session_id).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "error": "对话线程不存在",
                    "code": "session_not_found",
                })),
            );
        }
        Err(err) => {
            warn!(
                "runtime guidance session lookup failed: session_id={} detail={}",
                session_id, err
            );
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "success": false,
                    "error": "查询对话线程失败",
                    "code": "session_lookup_failed",
                })),
            );
        }
    };
    let session_user_id = target_session.user_id.as_deref().unwrap_or_default().trim();
    if session_user_id.is_empty() || session_user_id != auth_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "success": false,
                "error": "对话线程不属于当前用户",
                "code": "user_scope_forbidden",
            })),
        );
    }
    if let Some(requested_project_id) = requested_project_id.as_deref() {
        let session_project_id = target_session
            .project_id
            .clone()
            .or_else(|| project_id_from_metadata(target_session.metadata.as_ref()));
        let requested_scope = normalize_project_scope_id(Some(requested_project_id));
        let session_scope = normalize_project_scope_id(session_project_id.as_deref());
        if requested_scope != session_scope {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "success": false,
                    "error": "对话线程项目不匹配，已阻止跨项目引导",
                    "code": "project_scope_mismatch",
                    "session_project_id": session_scope,
                    "requested_project_id": requested_scope,
                })),
            );
        }
    }

    if abort_registry::is_aborted(session_id) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "success": false,
                "error": "当前轮次已停止，不再接收引导",
                "code": "turn_not_running",
            })),
        );
    }

    let enqueue_result = runtime_guidance_manager().enqueue_guidance(session_id, &turn_id, content);
    let guidance_item = match enqueue_result {
        Ok(item) => item,
        Err(EnqueueGuidanceError::TurnNotRunning) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "success": false,
                    "error": "当前轮次未运行或已结束",
                    "code": "turn_not_running",
                })),
            );
        }
    };

    let pending_count = runtime_guidance_manager().pending_count(session_id, &turn_id);
    let guidance_id = guidance_item.guidance_id.clone();

    let metadata = json!({
        "conversation_turn_id": turn_id,
        "hidden": true,
        "runtime_guidance": {
            "guidance_id": guidance_item.guidance_id,
            "status": "queued",
            "created_at": guidance_item.created_at,
        }
    });
    let message_manager = MessageManager::new();
    if let Err(err) = message_manager
        .save_user_message(
            session_id,
            content,
            Some(guidance_id.clone()),
            Some("runtime_guidance".to_string()),
            Some("runtime_guidance".to_string()),
            Some(metadata),
        )
        .await
    {
        warn!(
            "persist runtime guidance failed: session_id={} turn_id={} guidance_id={} detail={}",
            session_id, turn_id, guidance_id, err
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "conversation_id": session_id,
            "guidance_id": guidance_id,
            "status": "queued",
            "pending_count": pending_count,
            "turn_id": turn_id,
        })),
    )
}

async fn stream_chat_v3(sender: SseSender, req: ChatStreamRequest) {
    let session_id = req.conversation_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let cfg = Config::get();

    send_start_event(&sender, &session_id);

    maybe_spawn_session_title_rename(true, &session_id, &content, 30);

    let model_cfg: Value = req.ai_model_config.clone().unwrap_or_else(|| json!({}));
    if model_cfg
        .get("supports_responses")
        .and_then(|v| v.as_bool())
        != Some(true)
    {
        send_error_event(&sender, "当前模型未启用 Responses API");
        sender.send_done();
        return;
    }

    let model_runtime = resolve_chat_model_config(
        &model_cfg,
        "gpt-4o",
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        req.reasoning_enabled,
        true,
    );

    let mut ai_server = AiServer::new(
        model_runtime.api_key.clone(),
        model_runtime.base_url.clone(),
        model_runtime.model.clone(),
        model_runtime.temperature,
        McpToolExecute::new(Vec::new(), Vec::new(), Vec::new()),
    );
    let runtime_context = resolve_chat_stream_context(
        &session_id,
        &content,
        &req,
        model_runtime.system_prompt.clone(),
        model_runtime.use_active_system_context,
    )
    .await;
    if runtime_context.base_system_prompt.is_some() {
        ai_server.set_system_prompt(runtime_context.base_system_prompt.clone());
    }
    let prefixed_input_items = build_prefixed_input_items(&[
        runtime_context.contact_system_prompt.as_deref(),
        runtime_context.tool_routing_system_prompt.as_deref(),
        runtime_context.command_system_prompt.as_deref(),
    ]);

    let (http_servers, stdio_servers, builtin_servers) = runtime_context.mcp_server_bundle.clone();
    let use_tools = runtime_context.use_tools;
    let mut mcp_exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    if use_tools {
        let _ = if model_runtime.use_codex_gateway_mcp_passthrough {
            mcp_exec.init_builtin_only().await
        } else {
            mcp_exec.init().await
        };
    }
    let unavailable_tools = mcp_exec.get_unavailable_tools();
    send_tools_unavailable_event(&sender, unavailable_tools.as_slice());
    let mcp_tool_metadata = mcp_exec.tool_metadata.clone();
    ai_server.set_mcp_tool_execute(mcp_exec);

    let effective_settings = get_effective_user_settings(runtime_context.effective_user_id.clone())
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

    let callback_bundle = build_v3_callbacks(&sender, &session_id, true);
    let mut callbacks = callback_bundle.callbacks.clone();
    wire_implicit_command_tracking(
        &mut callbacks,
        runtime_context.selected_commands_for_snapshot.clone(),
    );
    let chunk_sent = callback_bundle.chunk_sent;

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);
    let user_message_id = Uuid::new_v4().to_string();
    let resolved_turn_id =
        normalize_turn_id(req.turn_id.as_deref()).unwrap_or_else(|| user_message_id.clone());
    runtime_guidance_manager().register_active_turn(&session_id, &resolved_turn_id);
    if let Err(err) = sync_chat_turn_snapshot(
        &session_id,
        &resolved_turn_id,
        "running",
        Some(user_message_id.clone()),
        model_runtime.model.as_str(),
        model_runtime.provider.as_str(),
        &mcp_tool_metadata,
        &runtime_context,
    )
    .await
    {
        warn!(
            "sync running turn snapshot failed: session_id={}, turn_id={}, detail={}",
            session_id, resolved_turn_id, err
        );
    }

    let result = ai_server
        .chat(
            &session_id,
            &content,
            ChatOptions {
                model: Some(model_runtime.model.clone()),
                provider: Some(model_runtime.provider.clone()),
                thinking_level: model_runtime.thinking_level.clone(),
                supports_responses: Some(model_runtime.supports_responses),
                temperature: Some(model_runtime.temperature),
                max_tokens,
                use_tools: Some(use_tools),
                attachments: Some(att),
                supports_images: Some(model_runtime.supports_images),
                reasoning_enabled: Some(model_runtime.effective_reasoning),
                callbacks: Some(callbacks),
                turn_id: Some(resolved_turn_id.clone()),
                user_message_id: Some(user_message_id.clone()),
                message_mode: Some("model".to_string()),
                message_source: Some(model_runtime.model.clone()),
                prefixed_input_items,
                request_cwd: if model_runtime.use_codex_gateway_mcp_passthrough {
                    runtime_context.resolved_project_root.clone()
                } else {
                    None
                },
                use_codex_gateway_mcp_passthrough: Some(
                    model_runtime.use_codex_gateway_mcp_passthrough,
                ),
            },
        )
        .await;

    if let Err(err) = sync_chat_turn_snapshot(
        &session_id,
        &resolved_turn_id,
        if result.is_ok() {
            "completed"
        } else {
            "failed"
        },
        Some(user_message_id.clone()),
        model_runtime.model.as_str(),
        model_runtime.provider.as_str(),
        &mcp_tool_metadata,
        &runtime_context,
    )
    .await
    {
        warn!(
            "sync completed turn snapshot failed: session_id={}, turn_id={}, detail={}",
            session_id, resolved_turn_id, err
        );
    }

    let should_send_done = handle_chat_result(
        &sender,
        &session_id,
        Some(&chunk_sent),
        Some(&callback_bundle.streamed_content),
        result,
        || log_chat_cancelled(&session_id),
        |err| log_chat_error(err),
    );
    if should_send_done {
        sender.send_done();
    }
    runtime_guidance_manager().close_turn(&session_id, &resolved_turn_id);
}

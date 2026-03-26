use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::ai_settings::chat_max_tokens_from_settings;
use crate::core::auth::AuthUser;
use crate::core::chat_context::{
    maybe_spawn_session_title_rename, resolve_effective_user_id, resolve_system_prompt,
};
use crate::core::chat_runtime::{
    compose_contact_command_system_prompt, compose_contact_system_prompt,
    contact_agent_id_from_metadata, contact_id_from_metadata, enabled_mcp_ids_from_metadata,
    mcp_enabled_from_metadata, normalize_id, parse_contact_command_invocation,
    parse_implicit_command_selections_from_tools_end, project_id_from_metadata,
    project_root_from_metadata, resolve_project_runtime,
};
use crate::core::chat_stream::{
    build_v3_callbacks, handle_chat_result, send_error_event, send_start_event,
};
use crate::core::mcp_runtime::{
    contact_agent_command_reader_server, contact_agent_plugin_reader_server,
    contact_agent_skill_reader_server, has_any_mcp_server, load_mcp_servers_by_selection,
    normalize_mcp_ids,
};
use crate::core::turn_runtime_snapshot::{
    build_turn_runtime_snapshot_payload, BuildTurnRuntimeSnapshotInput,
};
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
struct ChatRequest {
    session_id: Option<String>,
    content: Option<String>,
    ai_model_config: Option<Value>,
    user_id: Option<String>,
    attachments: Option<Vec<Value>>,
    reasoning_enabled: Option<bool>,
    turn_id: Option<String>,
    contact_agent_id: Option<String>,
    project_id: Option<String>,
    project_root: Option<String>,
    mcp_enabled: Option<bool>,
    enabled_mcp_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuntimeGuidanceRequest {
    session_id: Option<String>,
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
            "/api/agent_v3/session/:session_id/reset",
            post(reset_session),
        )
}

async fn agent_chat_stream(
    auth: AuthUser,
    Json(mut req): Json<ChatRequest>,
) -> Result<
    axum::response::Sse<
        impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    (StatusCode, Json<Value>),
> {
    if let Err(err) = ensure_and_set_user_id(&mut req.user_id, &auth) {
        return Err(err);
    }
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let has_text_content = !content.trim().is_empty();
    let has_attachments = req
        .attachments
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if session_id.is_empty() || (!has_text_content && !has_attachments) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "session_id 不能为空，且 content 与 attachments 不能同时为空"})),
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
    memory_server_client::spawn_with_current_access_token(stream_chat_v3(sender, req));
    Ok(sse)
}

async fn agent_tools(auth: AuthUser, Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let (http_servers, stdio_servers, builtin_servers) =
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
        "timestamp": crate::core::time::now_rfc3339(),
        "openai": {
            "configured": !cfg.openai_api_key.is_empty(),
            "base_url": cfg.openai_base_url.clone()
        }
    }))
}

async fn reset_session(Path(session_id): Path<String>) -> Json<Value> {
    let _ = memory_server_client::delete_messages_by_session(&session_id).await;
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

async fn submit_runtime_guidance(
    auth: AuthUser,
    Json(req): Json<RuntimeGuidanceRequest>,
) -> (StatusCode, Json<Value>) {
    const CONTENT_MAX_LEN: usize = 1000;

    let session_id = req
        .session_id
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
                "error": "session_id / turn_id / content 不能为空",
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
                    "error": "会话不存在",
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
                    "error": "查询会话失败",
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
                "error": "会话不属于当前用户",
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
                    "error": "会话项目不匹配，已阻止跨项目引导",
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
            "guidance_id": guidance_id,
            "status": "queued",
            "pending_count": pending_count,
            "turn_id": turn_id,
        })),
    )
}

async fn stream_chat_v3(sender: SseSender, req: ChatRequest) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let cfg = Config::get();

    send_start_event(&sender, &session_id);

    maybe_spawn_session_title_rename(true, &session_id, &content, 30);

    let model_cfg = req.ai_model_config.unwrap_or_else(|| json!({}));
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
    let memory_session = memory_server_client::get_session_by_id(&session_id)
        .await
        .ok()
        .flatten();
    let session_metadata = memory_session
        .as_ref()
        .and_then(|session| session.metadata.as_ref());

    let mut ai_server = AiServer::new(
        model_runtime.api_key.clone(),
        model_runtime.base_url.clone(),
        model_runtime.model.clone(),
        model_runtime.temperature,
        McpToolExecute::new(Vec::new(), Vec::new(), Vec::new()),
    );

    let effective_user_id = resolve_effective_user_id(req.user_id.clone(), &session_id).await;
    let mut contact_agent_id = normalize_id(req.contact_agent_id)
        .or_else(|| contact_agent_id_from_metadata(session_metadata))
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.selected_agent_id.clone()))
        });
    if contact_agent_id.is_none() {
        if let Some(contact_id) = contact_id_from_metadata(session_metadata) {
            if let Ok(contacts) = memory_server_client::list_memory_contacts(
                effective_user_id.as_deref(),
                Some(500),
                0,
            )
            .await
            {
                if let Some(contact) = contacts
                    .iter()
                    .find(|item| item.id.trim() == contact_id.as_str())
                {
                    contact_agent_id = normalize_id(Some(contact.agent_id.clone()));
                    if let Some(agent_id) = contact_agent_id.as_deref() {
                        warn!(
                            "resolved contact_agent_id from contact_id: session_id={} contact_id={} contact_agent_id={}",
                            session_id, contact_id, agent_id
                        );
                    }
                }
            }
        }
    }

    let contact_runtime_context = match contact_agent_id.as_deref() {
        Some(agent_id) => {
            match memory_server_client::get_memory_agent_runtime_context(agent_id).await {
                Ok(value) => value,
                Err(err) => {
                    warn!(
                    "load contact runtime context failed: session_id={} contact_agent_id={} detail={}",
                    session_id, agent_id, err
                );
                    None
                }
            }
        }
        None => None,
    };
    if contact_agent_id.is_some() && contact_runtime_context.is_none() {
        warn!(
            "contact runtime context missing: session_id={} contact_agent_id={}",
            session_id,
            contact_agent_id.as_deref().unwrap_or_default()
        );
    }
    let base_system_prompt = resolve_system_prompt(
        model_runtime.system_prompt.clone(),
        model_runtime.use_active_system_context,
        effective_user_id.clone(),
    )
    .await;
    let contact_system_prompt = compose_contact_system_prompt(contact_runtime_context.as_ref());
    let selected_command =
        parse_contact_command_invocation(content.as_str(), contact_runtime_context.as_ref());
    let command_system_prompt = compose_contact_command_system_prompt(selected_command.as_ref());
    let selected_commands_for_snapshot = Arc::new(Mutex::new(
        selected_command
            .as_ref()
            .map(|command| {
                vec![
                    memory_server_client::TurnRuntimeSnapshotSelectedCommandDto {
                        command_ref: Some(command.command_ref.clone()),
                        name: Some(command.name.clone()),
                        plugin_source: command.plugin_source.clone(),
                        source_path: command.source_path.clone(),
                        trigger: Some("explicit".to_string()),
                        arguments: command.arguments.clone(),
                    },
                ]
            })
            .unwrap_or_default(),
    ));
    if base_system_prompt.is_some() {
        ai_server.set_system_prompt(base_system_prompt.clone());
    }
    let mut prefixed_input_items_vec = Vec::new();
    if let Some(prompt) = contact_system_prompt.as_ref() {
        prefixed_input_items_vec.push(json!({
            "type": "message",
            "role": "system",
            "content": [
                {
                    "type": "input_text",
                    "text": prompt,
                }
            ]
        }));
    }
    if let Some(prompt) = command_system_prompt.as_ref() {
        prefixed_input_items_vec.push(json!({
            "type": "message",
            "role": "system",
            "content": [
                {
                    "type": "input_text",
                    "text": prompt,
                }
            ]
        }));
    }
    let prefixed_input_items = if prefixed_input_items_vec.is_empty() {
        None
    } else {
        Some(prefixed_input_items_vec)
    };

    let requested_project_id = normalize_id(req.project_id)
        .or_else(|| project_id_from_metadata(session_metadata))
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.project_id.clone()))
        });
    let requested_project_root =
        normalize_id(req.project_root).or_else(|| project_root_from_metadata(session_metadata));
    let (resolved_project_id, resolved_project_root) = resolve_project_runtime(
        effective_user_id.as_deref(),
        requested_project_id,
        requested_project_root,
    )
    .await;
    let requested_mcp_ids = req
        .enabled_mcp_ids
        .unwrap_or_else(|| enabled_mcp_ids_from_metadata(session_metadata));
    let normalized_mcp_ids = normalize_mcp_ids(&requested_mcp_ids);
    let enabled_mcp_ids_for_snapshot = normalized_mcp_ids.clone();
    let mcp_enabled = req
        .mcp_enabled
        .or_else(|| mcp_enabled_from_metadata(session_metadata))
        .unwrap_or(true);
    let (http_servers, stdio_servers, mut builtin_servers) = if mcp_enabled {
        load_mcp_servers_by_selection(
            effective_user_id.clone(),
            !normalized_mcp_ids.is_empty(),
            normalized_mcp_ids,
            resolved_project_root.as_deref(),
            resolved_project_id.as_deref(),
        )
        .await
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    if let Some(agent_id) = contact_runtime_context
        .as_ref()
        .map(|context| context.agent_id.as_str())
    {
        if let Some(server) = contact_agent_skill_reader_server(
            effective_user_id.clone(),
            resolved_project_id.clone(),
            agent_id,
        ) {
            builtin_servers.push(server);
        }
        if let Some(server) = contact_agent_command_reader_server(
            effective_user_id.clone(),
            resolved_project_id.clone(),
            agent_id,
        ) {
            builtin_servers.push(server);
        }
        if let Some(server) = contact_agent_plugin_reader_server(
            effective_user_id.clone(),
            resolved_project_id.clone(),
            agent_id,
        ) {
            builtin_servers.push(server);
        }
    }
    let use_tools = has_any_mcp_server(&http_servers, &stdio_servers, &builtin_servers);
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
    let mcp_tool_metadata = mcp_exec.tool_metadata.clone();
    ai_server.set_mcp_tool_execute(mcp_exec);

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

    let callback_bundle = build_v3_callbacks(&sender, &session_id, true);
    let mut callbacks = callback_bundle.callbacks.clone();
    let original_on_tools_end = callbacks.on_tools_end.clone();
    let selected_commands_for_snapshot_on_tools_end = selected_commands_for_snapshot.clone();
    callbacks.on_tools_end = Some(Arc::new(move |result: Value| {
        let implicit_items = parse_implicit_command_selections_from_tools_end(&result);
        if !implicit_items.is_empty() {
            if let Ok(mut snapshot_items) = selected_commands_for_snapshot_on_tools_end.lock() {
                for item in implicit_items {
                    snapshot_items.push(
                        memory_server_client::TurnRuntimeSnapshotSelectedCommandDto {
                            command_ref: item.command_ref,
                            name: item.name,
                            plugin_source: item.plugin_source,
                            source_path: item.source_path,
                            trigger: Some("implicit".to_string()),
                            arguments: None,
                        },
                    );
                }
            }
        }
        if let Some(callback) = original_on_tools_end.as_ref() {
            callback(result);
        }
    }));
    let chunk_sent = callback_bundle.chunk_sent;

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);
    let memory_summary_prompt = memory_server_client::compose_context(&session_id, 2)
        .await
        .ok()
        .and_then(|payload| payload.0)
        .and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    let user_message_id = Uuid::new_v4().to_string();
    let resolved_turn_id =
        normalize_turn_id(req.turn_id.as_deref()).unwrap_or_else(|| user_message_id.clone());
    runtime_guidance_manager().register_active_turn(&session_id, &resolved_turn_id);
    let running_selected_commands = selected_commands_for_snapshot
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    let running_snapshot_payload =
        build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
            user_message_id: Some(user_message_id.clone()),
            status: "running",
            base_system_prompt: base_system_prompt.as_deref(),
            contact_system_prompt: contact_system_prompt.as_deref(),
            memory_summary_prompt: memory_summary_prompt.as_deref(),
            tools: &mcp_tool_metadata,
            model: Some(model_runtime.model.as_str()),
            provider: Some(model_runtime.provider.as_str()),
            contact_agent_id: contact_agent_id.as_deref(),
            project_id: resolved_project_id.as_deref(),
            project_root: resolved_project_root.as_deref(),
            mcp_enabled,
            enabled_mcp_ids: &enabled_mcp_ids_for_snapshot,
            selected_commands: running_selected_commands.as_slice(),
        });
    if let Err(err) = memory_server_client::sync_turn_runtime_snapshot(
        &session_id,
        &resolved_turn_id,
        &running_snapshot_payload,
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
                user_message_id: Some(user_message_id),
                message_mode: Some("model".to_string()),
                message_source: Some(model_runtime.model.clone()),
                prefixed_input_items,
                request_cwd: if model_runtime.use_codex_gateway_mcp_passthrough {
                    resolved_project_root.clone()
                } else {
                    None
                },
                use_codex_gateway_mcp_passthrough: Some(
                    model_runtime.use_codex_gateway_mcp_passthrough,
                ),
            },
        )
        .await;

    let completed_selected_commands = selected_commands_for_snapshot
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    let completed_snapshot_payload =
        build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
            user_message_id: running_snapshot_payload.user_message_id.clone(),
            status: if result.is_ok() {
                "completed"
            } else {
                "failed"
            },
            base_system_prompt: base_system_prompt.as_deref(),
            contact_system_prompt: contact_system_prompt.as_deref(),
            memory_summary_prompt: memory_summary_prompt.as_deref(),
            tools: &mcp_tool_metadata,
            model: Some(model_runtime.model.as_str()),
            provider: Some(model_runtime.provider.as_str()),
            contact_agent_id: contact_agent_id.as_deref(),
            project_id: resolved_project_id.as_deref(),
            project_root: resolved_project_root.as_deref(),
            mcp_enabled,
            enabled_mcp_ids: &enabled_mcp_ids_for_snapshot,
            selected_commands: completed_selected_commands.as_slice(),
        });
    if let Err(err) = memory_server_client::sync_turn_runtime_snapshot(
        &session_id,
        &resolved_turn_id,
        &completed_snapshot_payload,
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

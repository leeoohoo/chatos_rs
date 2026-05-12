#[path = "chat_v3/runtime_guidance.rs"]
mod runtime_guidance;
#[path = "chat_v3/tools_panel.rs"]
mod tools_panel;

use axum::http::StatusCode;
use axum::{
    extract::Path,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::api::chat_stream_common::{
    resolve_chat_stream_context, sync_chat_turn_snapshot, validate_chat_stream_request,
    wire_implicit_command_tracking, ChatStreamRequest,
};
use crate::api::conversation_semantics::extract_conversation_scope_id;
use crate::config::Config;
use crate::core::ai_settings::chat_max_tokens_from_settings;
use crate::core::auth::AuthUser;
use crate::core::builtin_mcp_prompt::compose_effective_builtin_mcp_system_prompt;
use crate::core::internal_context_locale::internal_context_locale_from_settings;
use crate::core::chat_context::maybe_spawn_session_title_rename;
use crate::core::chat_stream::{
    build_v3_callbacks, enrich_chat_result_with_persisted_messages, handle_chat_result, send_error_event, send_start_event,
    send_tools_unavailable_event, ChatEventSink, ChatRealtimeStreamContext,
};
use crate::core::user_scope::ensure_and_set_user_id;
use crate::services::ai_common::normalize_turn_id;
use crate::services::access_token_scope;
use crate::services::chatos_sessions;
use crate::services::model_runtime_resolver::resolve_model_runtime_for_request;
use crate::services::runtime_guidance_manager::runtime_guidance_manager;
use crate::services::task_board_prompt::build_runtime_prefixed_input_items;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v3::ai_server::{AiServer, ChatOptions};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::log_helpers::{log_chat_begin, log_chat_cancelled, log_chat_error};
use crate::utils::sse::{sse_channel, SseSender};
use tracing::warn;
use uuid::Uuid;
use self::runtime_guidance::submit_runtime_guidance;
use self::tools_panel::{agent_status, agent_tools};

pub fn router() -> Router {
    Router::new()
        .route("/api/agent_v3/chat/stream", post(agent_chat_stream))
        .route("/api/agent_v3/chat/send", post(agent_chat_send))
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
    validate_chat_stream_request(&req, true).await?;
    let conversation_id = req.conversation_id.clone().unwrap_or_default();

    abort_registry::reset(&conversation_id);
    let (sse, sender) = sse_channel();
    access_token_scope::spawn_with_current_access_token(stream_chat_v3(Some(sender), req));
    Ok(sse)
}

async fn agent_chat_send(
    auth: AuthUser,
    Json(mut req): Json<ChatStreamRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    if let Err(err) = ensure_and_set_user_id(&mut req.user_id, &auth) {
        return Err(err);
    }
    validate_chat_stream_request(&req, true).await?;
    let conversation_id = req.conversation_id.clone().unwrap_or_default();
    let accepted_turn_id = normalize_turn_id(req.turn_id.as_deref());

    abort_registry::reset(&conversation_id);
    access_token_scope::spawn_with_current_access_token(stream_chat_v3(None, req));

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": accepted_turn_id,
        })),
    ))
}

async fn reset_conversation(Path(conversation_id): Path<String>) -> Json<Value> {
    let _ = chatos_sessions::delete_messages_by_session(&conversation_id).await;
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

async fn stream_chat_v3(sender: Option<SseSender>, req: ChatStreamRequest) {
    let session_id = req.conversation_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let initial_turn_id = normalize_turn_id(req.turn_id.as_deref());
    let initial_sink = ChatEventSink::new(
        sender.clone(),
        Some(ChatRealtimeStreamContext {
            user_id: req.user_id.clone(),
            conversation_id: Some(session_id.clone()),
            conversation_turn_id: initial_turn_id.clone(),
            project_id: req.project_id.clone(),
            user_message_id: None,
        }),
    );
    if let Err(err) = Config::try_get() {
        send_error_event(&initial_sink, format!("服务配置未初始化: {err}").as_str(), None);
        initial_sink.send_done();
        return;
    }

    send_start_event(&initial_sink, &session_id);

    maybe_spawn_session_title_rename(true, &session_id, &content, 30);

    let model_runtime = match resolve_model_runtime_for_request(
        req.model_config_id.as_deref(),
        req.ai_model_config.as_ref(),
        req.conversation_id.as_deref(),
        req.user_id.as_deref(),
        "gpt-4o",
        req.reasoning_enabled,
        true,
    )
    .await
    {
        Ok(runtime) => runtime,
        Err(err) => {
            send_error_event(&initial_sink, format!("解析模型配置失败: {err}").as_str(), None);
            initial_sink.send_done();
            return;
        }
    };
    if !model_runtime.supports_responses {
        send_error_event(&initial_sink, "当前模型未启用 Responses API", None);
        initial_sink.send_done();
        return;
    }

    let mut ai_server = AiServer::new(
        model_runtime.api_key.clone(),
        model_runtime.base_url.clone(),
        model_runtime.model.clone(),
        model_runtime.temperature,
        McpToolExecute::new(Vec::new(), Vec::new(), Vec::new()),
    );
    let effective_settings = get_effective_user_settings(req.user_id.clone())
        .await
        .unwrap_or_else(|_| json!({}));
    let internal_context_locale = internal_context_locale_from_settings(&effective_settings);

    let mut runtime_context = resolve_chat_stream_context(
        &session_id,
        &content,
        &req,
        model_runtime.system_prompt.clone(),
        model_runtime.use_active_system_context,
        internal_context_locale,
    )
    .await;
    if runtime_context.base_system_prompt.is_some() {
        ai_server.set_system_prompt(runtime_context.base_system_prompt.clone());
    }

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
    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);
    let user_message_id = Uuid::new_v4().to_string();
    let resolved_turn_id = initial_turn_id.unwrap_or_else(|| user_message_id.clone());
    let unavailable_tools = mcp_exec.get_unavailable_tools();
    runtime_context.builtin_mcp_system_prompt = compose_effective_builtin_mcp_system_prompt(
        builtin_servers.as_slice(),
        mcp_exec.tool_metadata(),
        unavailable_tools.as_slice(),
        runtime_context.internal_context_locale,
    );
    let prefixed_input_items = build_runtime_prefixed_input_items(
        &session_id,
        Some(resolved_turn_id.as_str()),
        runtime_context.internal_context_locale,
        runtime_context.contact_system_prompt.as_deref(),
        runtime_context.builtin_mcp_system_prompt.as_deref(),
        runtime_context.command_system_prompt.as_deref(),
    )
    .await;
    let sink = ChatEventSink::new(
        sender.clone(),
        Some(ChatRealtimeStreamContext {
            user_id: req.user_id.clone(),
            conversation_id: Some(session_id.clone()),
            conversation_turn_id: Some(resolved_turn_id.clone()),
            project_id: req.project_id.clone(),
            user_message_id: Some(user_message_id.clone()),
        }),
    );
    send_tools_unavailable_event(&sink, unavailable_tools.as_slice());
    let mcp_tool_metadata = mcp_exec.tool_metadata().clone();
    ai_server.set_mcp_tool_execute(mcp_exec);
    ai_server.ai_client.set_task_board_refresh_context(
        Some(session_id.clone()),
        Some(resolved_turn_id.clone()),
        runtime_context.internal_context_locale,
        runtime_context.contact_system_prompt.clone(),
        runtime_context.builtin_mcp_system_prompt.clone(),
        runtime_context.command_system_prompt.clone(),
    );

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

    let callback_bundle = build_v3_callbacks(&sink, &session_id, true);
    let mut callbacks = callback_bundle.callbacks.clone();
    wire_implicit_command_tracking(
        &mut callbacks,
        runtime_context.selected_commands_for_snapshot.clone(),
    );
    let chunk_sent = callback_bundle.chunk_sent;
    runtime_guidance_manager().register_active_turn(&session_id, &resolved_turn_id);
    if let Err(err) = sync_chat_turn_snapshot(
        &session_id,
        &resolved_turn_id,
        "running",
        Some(user_message_id.clone()),
        model_runtime.model.as_str(),
        model_runtime.provider.as_str(),
        &mcp_tool_metadata,
        unavailable_tools.as_slice(),
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
        unavailable_tools.as_slice(),
        &runtime_context,
    )
    .await
    {
        warn!(
            "sync completed turn snapshot failed: session_id={}, turn_id={}, detail={}",
            session_id, resolved_turn_id, err
        );
    }

    let result = match result {
        Ok(value) => Ok(
            enrich_chat_result_with_persisted_messages(
                &session_id,
                Some(resolved_turn_id.as_str()),
                Some(user_message_id.as_str()),
                value,
            )
            .await,
        ),
        Err(error) => Err(error),
    };

    let should_send_done = handle_chat_result(
        &sink,
        &session_id,
        Some(resolved_turn_id.as_str()),
        Some(user_message_id.as_str()),
        Some(&chunk_sent),
        Some(&callback_bundle.streamed_content),
        result,
        || log_chat_cancelled(&session_id),
        |err| log_chat_error(err),
    )
    .await;
    if should_send_done {
        sink.send_done();
    }
    runtime_guidance_manager().close_turn(&session_id, &resolved_turn_id);
}

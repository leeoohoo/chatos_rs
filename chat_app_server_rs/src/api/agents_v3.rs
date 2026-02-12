use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::task;

use crate::config::Config;
use crate::core::agent_runtime::{load_enabled_agent_model, AgentModelLoadError};
use crate::core::ai_settings::{chat_max_tokens_from_settings, effective_reasoning_enabled};
use crate::core::chat_context::maybe_spawn_session_title_rename;
use crate::core::chat_stream::{
    build_v3_callbacks, handle_chat_result, send_error_event, send_start_event,
};
use crate::core::mcp_runtime::{
    has_any_mcp_server, load_mcp_servers_by_selection, normalize_mcp_ids,
};
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v3::ai_server::{AiServer, ChatOptions};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::log_helpers::log_chat_error;
use crate::utils::sse::{sse_channel, SseSender};
use crate::utils::workspace::resolve_workspace_dir;

#[derive(Debug, Deserialize)]
struct AgentChatRequest {
    session_id: Option<String>,
    content: Option<String>,
    agent_id: Option<String>,
    user_id: Option<String>,
    attachments: Option<Vec<Value>>,
    reasoning_enabled: Option<bool>,
}

pub fn router() -> Router {
    Router::new().route("/api/agent_v3/agents/chat/stream", post(chat_stream))
}

async fn chat_stream(
    Json(req): Json<AgentChatRequest>,
) -> Result<
    axum::response::Sse<
        impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    (StatusCode, Json<Value>),
> {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let agent_id = req.agent_id.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() || agent_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "session_id, content 和 agent_id 为必填项"})),
        ));
    }

    abort_registry::reset(&session_id);
    let (sse, sender) = sse_channel();
    task::spawn(stream_agent_v3(sender, req));
    Ok(sse)
}

async fn stream_agent_v3(sender: SseSender, req: AgentChatRequest) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let agent_id = req.agent_id.clone().unwrap_or_default();
    let cfg = Config::get();

    send_start_event(&sender, &session_id);
    maybe_spawn_session_title_rename(true, &session_id, &content, 30);

    let resolved = match load_enabled_agent_model(&agent_id).await {
        Ok(value) => value,
        Err(AgentModelLoadError::AgentUnavailable) => {
            send_error_event(&sender, "Agent 不存在或已禁用");
            sender.send_done();
            return;
        }
        Err(AgentModelLoadError::ModelUnavailable) => {
            send_error_event(&sender, "模型配置不可用或未启用");
            sender.send_done();
            return;
        }
        Err(AgentModelLoadError::Repository(err)) => {
            send_error_event(&sender, &err);
            sender.send_done();
            return;
        }
    };

    let crate::core::agent_runtime::EnabledAgentModel {
        agent,
        model: model_cfg,
        system_prompt,
    } = resolved;

    if model_cfg.supports_responses != true {
        send_error_event(&sender, "当前模型未启用 Responses API");
        sender.send_done();
        return;
    }

    let effective_user_id = req.user_id.clone().or(agent.user_id.clone());
    let supports_images = model_cfg.supports_images;
    let supports_reasoning = model_cfg.supports_reasoning;
    let reasoning_enabled = req.reasoning_enabled.unwrap_or(true);
    let effective_reasoning = effective_reasoning_enabled(
        supports_reasoning,
        model_cfg.thinking_level.as_deref(),
        reasoning_enabled,
    );

    let mcp_ids = normalize_mcp_ids(&agent.mcp_config_ids);
    let use_tools = !mcp_ids.is_empty();
    let workspace_dir = resolve_workspace_dir(agent.workspace_dir.as_deref());
    let workspace_dir_opt = if workspace_dir.trim().is_empty() {
        None
    } else {
        Some(workspace_dir.as_str())
    };
    let (http_servers, stdio_servers, builtin_servers) = load_mcp_servers_by_selection(
        effective_user_id.clone(),
        true,
        mcp_ids.clone(),
        workspace_dir_opt,
        agent.project_id.as_deref(),
    )
    .await;
    let mut mcp_exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    if use_tools && has_any_mcp_server(&http_servers, &stdio_servers, &builtin_servers) {
        let _ = mcp_exec.init().await;
    }

    let mut ai_server = AiServer::new(
        model_cfg
            .api_key
            .clone()
            .unwrap_or_else(|| cfg.openai_api_key.clone()),
        model_cfg
            .base_url
            .clone()
            .unwrap_or_else(|| cfg.openai_base_url.clone()),
        model_cfg.model.clone(),
        0.7,
        mcp_exec,
    );

    if let Some(prompt) = system_prompt {
        ai_server.set_system_prompt(Some(prompt));
    }

    let effective_settings = get_effective_user_settings(effective_user_id.clone())
        .await
        .unwrap_or_else(|_| json!({}));
    apply_settings_to_ai_client(&mut ai_server.ai_client, &effective_settings);
    let max_tokens = chat_max_tokens_from_settings(&effective_settings);

    let callback_bundle = build_v3_callbacks(&sender, &session_id, use_tools);
    let chunk_sent = callback_bundle.chunk_sent;

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);

    let result = ai_server
        .chat(
            &session_id,
            &content,
            ChatOptions {
                model: Some(model_cfg.model.clone()),
                provider: Some(model_cfg.provider.clone()),
                thinking_level: model_cfg.thinking_level.clone(),
                temperature: Some(0.7),
                max_tokens,
                use_tools: Some(use_tools),
                attachments: Some(att),
                supports_images: Some(supports_images),
                reasoning_enabled: Some(effective_reasoning),
                callbacks: Some(callback_bundle.callbacks),
            },
        )
        .await;

    let should_send_done = handle_chat_result(
        &sender,
        &session_id,
        Some(&chunk_sent),
        result,
        || {},
        |err| log_chat_error(err),
    );
    if should_send_done {
        sender.send_done();
    }
}

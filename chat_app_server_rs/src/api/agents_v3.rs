use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::task;

use crate::config::Config;
use crate::repositories::{
    agents as agents_repo, ai_model_configs as ai_repo, system_contexts as ctx_repo,
};
use crate::services::mcp_loader::load_mcp_configs_for_user;
use crate::services::session_title::maybe_rename_session_title;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v3::ai_client::AiClientCallbacks;
use crate::services::v3::ai_server::{AiServer, ChatOptions};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::events::Events;
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

    sender.send_json(&json!({ "type": Events::START, "timestamp": chrono::Utc::now().to_rfc3339(), "session_id": session_id }));
    if !session_id.is_empty() && !content.is_empty() {
        let sid = session_id.clone();
        let text = content.clone();
        tokio::spawn(async move {
            let _ = maybe_rename_session_title(&sid, &text, 30).await;
        });
    }

    let agent = match agents_repo::get_agent_by_id(&agent_id).await {
        Ok(Some(a)) if a.enabled => a,
        _ => {
            sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": "Agent 不存在或已禁用" } }));
            return;
        }
    };

    let model_cfg = match ai_repo::get_ai_model_config_by_id(&agent.ai_model_config_id).await {
        Ok(Some(m)) if m.enabled => m,
        _ => {
            sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": "模型配置不可用或未启用" } }));
            return;
        }
    };

    if model_cfg.supports_responses != true {
        sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": "当前模型未启用 Responses API" } }));
        return;
    }

    let mut system_prompt = None;
    if let Some(ctx_id) = agent.system_context_id.clone() {
        if let Ok(Some(ctx)) = ctx_repo::get_system_context_by_id(&ctx_id).await {
            if ctx.is_active {
                system_prompt = ctx.content;
            }
        }
    }

    let effective_user_id = req.user_id.clone().or(agent.user_id.clone());
    let supports_images = model_cfg.supports_images;
    let supports_reasoning = model_cfg.supports_reasoning;
    let reasoning_enabled = req.reasoning_enabled.unwrap_or(true);
    let effective_reasoning =
        (supports_reasoning || model_cfg.thinking_level.is_some()) && reasoning_enabled;

    let mcp_ids: Vec<String> = agent
        .mcp_config_ids
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();
    let use_tools = !mcp_ids.is_empty();
    let workspace_dir = resolve_workspace_dir(agent.workspace_dir.as_deref());
    let workspace_dir_opt = if workspace_dir.trim().is_empty() {
        None
    } else {
        Some(workspace_dir.as_str())
    };
    let (http_servers, mut stdio_servers, builtin_servers) = if use_tools {
        load_mcp_configs_for_user(
            effective_user_id.clone(),
            Some(mcp_ids.clone()),
            workspace_dir_opt,
            agent.project_id.as_deref(),
        )
        .await
        .unwrap_or((Vec::new(), Vec::new(), Vec::new()))
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    if !workspace_dir.trim().is_empty() {
        for server in stdio_servers.iter_mut() {
            if server
                .cwd
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                server.cwd = Some(workspace_dir.clone());
            }
        }
    }
    let mut mcp_exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    if use_tools
        && (!http_servers.is_empty() || !stdio_servers.is_empty() || !builtin_servers.is_empty())
    {
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
    let max_tokens = effective_settings
        .get("CHAT_MAX_TOKENS")
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0);

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
        on_tools_start: if use_tools {
            Some(std::sync::Arc::new(on_tools_start))
        } else {
            None
        },
        on_tools_stream: if use_tools {
            Some(std::sync::Arc::new(on_tools_stream))
        } else {
            None
        },
        on_tools_end: if use_tools {
            Some(std::sync::Arc::new(on_tools_end))
        } else {
            None
        },
    };

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

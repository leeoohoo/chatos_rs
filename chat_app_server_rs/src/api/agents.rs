use axum::{Router, Json, routing::{get, post}};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::task;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

use crate::models::agent::Agent;
use crate::repositories::agents as agents_repo;
use crate::services::v2::agent::{load_model_config_for_agent, run_chat};
use crate::services::v2::ai_client::AiClientCallbacks;
use crate::utils::attachments;
use crate::utils::events::Events;
use crate::utils::sse::{sse_channel, SseSender};
use crate::services::session_title::maybe_rename_session_title;
use crate::utils::abort_registry;
use crate::utils::workspace::{normalize_workspace_dir, sanitize_workspace_dir};

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentRequest {
    name: Option<String>,
    ai_model_config_id: Option<String>,
    system_context_id: Option<String>,
    description: Option<String>,
    user_id: Option<String>,
    enabled: Option<bool>,
    app_ids: Option<Vec<String>>,
    mcp_config_ids: Option<Value>,
    callable_agent_ids: Option<Value>,
    project_id: Option<String>,
    workspace_dir: Option<String>,
}

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
    Router::new()
        .route("/", get(list_agents).post(create_agent))
        .route("/:agent_id", get(get_agent).put(update_agent).delete(delete_agent))
        .route("/chat/stream", post(chat_stream))
}

async fn list_agents(axum::extract::Query(query): axum::extract::Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let agents = match agents_repo::list_agents(query.user_id).await {
        Ok(list) => list,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "获取智能体列表失败", "detail": err}))),
    };
    let mut out = Vec::new();
    for a in agents {
        let app_ids = match agents_repo::get_app_ids_for_agent(&a.id).await {
            Ok(ids) => ids,
            Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "获取智能体列表失败", "detail": err}))),
        };
        out.push(json!({
            "id": a.id,
            "name": a.name,
            "ai_model_config_id": a.ai_model_config_id,
            "system_context_id": a.system_context_id,
            "description": a.description,
            "user_id": a.user_id,
            "mcp_config_ids": a.mcp_config_ids,
            "callable_agent_ids": a.callable_agent_ids,
            "project_id": a.project_id,
            "workspace_dir": normalize_workspace_dir(a.workspace_dir.as_deref()),
            "enabled": a.enabled,
            "created_at": a.created_at,
            "updated_at": a.updated_at,
            "app_ids": app_ids
        }));
    }
    (StatusCode::OK, Json(Value::Array(out)))
}

async fn create_agent(Json(req): Json<AgentRequest>) -> (StatusCode, Json<Value>) {
    if req.name.is_none() || req.ai_model_config_id.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "name 和 ai_model_config_id 为必填项"})));
    }
    let id = Uuid::new_v4().to_string();
    let agent = Agent {
        id: id.clone(),
        name: req.name.unwrap(),
        ai_model_config_id: req.ai_model_config_id.unwrap(),
        system_context_id: req.system_context_id,
        description: req.description,
        user_id: req.user_id,
        mcp_config_ids: parse_id_list(&req.mcp_config_ids).unwrap_or_default(),
        callable_agent_ids: parse_id_list(&req.callable_agent_ids).unwrap_or_default(),
        project_id: req.project_id.and_then(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }),
        workspace_dir: sanitize_workspace_dir(req.workspace_dir),
        enabled: req.enabled.unwrap_or(true),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(err) = agents_repo::create_agent(&agent).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "创建智能体失败", "detail": err})));
    }
    if let Some(app_ids) = req.app_ids.clone() {
        if let Err(err) = agents_repo::set_app_ids_for_agent(&id, &app_ids).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "创建智能体失败", "detail": err})));
        }
    }
    let agent = match agents_repo::get_agent_by_id(&id).await {
        Ok(Some(agent)) => agent,
        Ok(None) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "创建智能体失败"}))),
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "创建智能体失败", "detail": err}))),
    };
    let app_ids = match agents_repo::get_app_ids_for_agent(&id).await {
        Ok(ids) => ids,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "创建智能体失败", "detail": err}))),
    };
    (StatusCode::CREATED, Json(agent_value(&agent, Some(app_ids))))
}

async fn get_agent(axum::extract::Path(agent_id): axum::extract::Path<String>) -> (StatusCode, Json<Value>) {
    let agent = match agents_repo::get_agent_by_id(&agent_id).await {
        Ok(agent) => agent,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "获取智能体失败", "detail": err}))),
    };
    if let Some(a) = agent {
        let app_ids = match agents_repo::get_app_ids_for_agent(&agent_id).await {
            Ok(ids) => ids,
            Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "获取智能体失败", "detail": err}))),
        };
        return (StatusCode::OK, Json(json!({
            "id": a.id,
            "name": a.name,
            "ai_model_config_id": a.ai_model_config_id,
            "system_context_id": a.system_context_id,
            "description": a.description,
            "user_id": a.user_id,
            "mcp_config_ids": a.mcp_config_ids,
            "callable_agent_ids": a.callable_agent_ids,
            "project_id": a.project_id,
            "workspace_dir": normalize_workspace_dir(a.workspace_dir.as_deref()),
            "enabled": a.enabled,
            "created_at": a.created_at,
            "updated_at": a.updated_at,
            "app_ids": app_ids
        })));
    }
    (StatusCode::NOT_FOUND, Json(json!({"error": "Agent 不存在"})))
}

async fn update_agent(axum::extract::Path(agent_id): axum::extract::Path<String>, Json(req): Json<AgentRequest>) -> (StatusCode, Json<Value>) {
    let existing = match agents_repo::get_agent_by_id(&agent_id).await {
        Ok(agent) => agent,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "更新智能体失败", "detail": err}))),
    };
    let Some(mut agent) = existing else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "Agent 不存在"})));
    };
    if let Some(v) = req.name { agent.name = v; }
    if let Some(v) = req.ai_model_config_id { agent.ai_model_config_id = v; }
    if let Some(v) = req.system_context_id { agent.system_context_id = Some(v); }
    if let Some(v) = req.description { agent.description = Some(v); }
    if let Some(v) = req.enabled { agent.enabled = v; }
    if let Some(v) = parse_id_list(&req.mcp_config_ids) { agent.mcp_config_ids = v; }
    if let Some(v) = parse_id_list(&req.callable_agent_ids) { agent.callable_agent_ids = v; }
    if let Some(v) = req.project_id {
        let trimmed = v.trim();
        agent.project_id = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
    }
    if let Some(v) = req.workspace_dir { agent.workspace_dir = sanitize_workspace_dir(Some(v)); }
    if let Err(err) = agents_repo::update_agent(&agent_id, &agent).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "更新智能体失败", "detail": err})));
    }
    if let Some(app_ids) = req.app_ids {
        if let Err(err) = agents_repo::set_app_ids_for_agent(&agent_id, &app_ids).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "更新智能体失败", "detail": err})));
        }
    }
    let agent = match agents_repo::get_agent_by_id(&agent_id).await {
        Ok(Some(agent)) => agent,
        Ok(None) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "更新智能体失败"}))),
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "更新智能体失败", "detail": err}))),
    };
    let app_ids = match agents_repo::get_app_ids_for_agent(&agent_id).await {
        Ok(ids) => ids,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "更新智能体失败", "detail": err}))),
    };
    (StatusCode::OK, Json(agent_value(&agent, Some(app_ids))))
}

async fn delete_agent(axum::extract::Path(agent_id): axum::extract::Path<String>) -> (StatusCode, Json<Value>) {
    let existing = match agents_repo::get_agent_by_id(&agent_id).await {
        Ok(agent) => agent,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "删除智能体失败", "detail": err}))),
    };
    if existing.is_none() {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "Agent 不存在"})));
    }
    if let Err(err) = agents_repo::delete_agent(&agent_id).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "删除智能体失败", "detail": err})));
    }
    (StatusCode::OK, Json(json!({"ok": true })))
}

async fn chat_stream(Json(req): Json<AgentChatRequest>) -> Result<axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, (StatusCode, Json<Value>)> {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let agent_id = req.agent_id.clone().unwrap_or_default();
    if session_id.is_empty() || content.is_empty() || agent_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "session_id, content 和 agent_id 为必填项"}))));
    }
    abort_registry::reset(&session_id);
    let (sse, sender) = sse_channel();
    task::spawn(stream_agent_chat(sender, req));
    Ok(sse)
}

async fn stream_agent_chat(sender: SseSender, req: AgentChatRequest) {
    let session_id = req.session_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let agent_id = req.agent_id.clone().unwrap_or_default();
    sender.send_json(&json!({ "type": Events::START, "timestamp": chrono::Utc::now().to_rfc3339(), "session_id": session_id }));
    if !session_id.is_empty() && !content.is_empty() {
        let sid = session_id.clone();
        let text = content.clone();
        tokio::spawn(async move {
            let _ = maybe_rename_session_title(&sid, &text, 30).await;
        });
    }

    let model_cfg = match load_model_config_for_agent(&agent_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            sender.send_json(&json!({ "type": Events::ERROR, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "error": err } }));
            sender.send_done();
            return;
        }
    };

    let chunk_sent = std::sync::Arc::new(AtomicBool::new(false));
    let sender_clone = sender.clone();
    let sid_clone = session_id.clone();
    let chunk_flag = chunk_sent.clone();
    let on_chunk = move |chunk: String| {
        if abort_registry::is_aborted(&sid_clone) { return; }
        chunk_flag.store(true, Ordering::Relaxed);
        sender_clone.send_json(&json!({ "type": Events::CHUNK, "timestamp": chrono::Utc::now().to_rfc3339(), "content": chunk }));
    };
    let sender_thinking = sender.clone();
    let sid_thinking = session_id.clone();
    let on_thinking = move |chunk: String| {
        if abort_registry::is_aborted(&sid_thinking) { return; }
        sender_thinking.send_json(&json!({ "type": Events::THINKING, "timestamp": chrono::Utc::now().to_rfc3339(), "content": chunk }));
    };
    let sender_tools = sender.clone();
    let sid_tools = session_id.clone();
    let on_tools_start = move |tool_calls: Value| {
        if abort_registry::is_aborted(&sid_tools) { return; }
        sender_tools.send_json(&json!({ "type": Events::TOOLS_START, "timestamp": chrono::Utc::now().to_rfc3339(), "data": { "tool_calls": tool_calls } }));
    };
    let sender_tools_stream = sender.clone();
    let sid_tools_stream = session_id.clone();
    let on_tools_stream = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_stream) { return; }
        sender_tools_stream.send_json(&json!({ "type": Events::TOOLS_STREAM, "timestamp": chrono::Utc::now().to_rfc3339(), "data": result }));
    };
    let sender_tools_end = sender.clone();
    let sid_tools_end = session_id.clone();
    let on_tools_end = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_end) { return; }
        sender_tools_end.send_json(&json!({ "type": Events::TOOLS_END, "timestamp": chrono::Utc::now().to_rfc3339(), "data": result }));
    };

    let sender_sum_start = sender.clone();
    let sid_sum_start = session_id.clone();
    let on_sum_start = move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_start) { return; }
        sender_sum_start.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_START, "timestamp": chrono::Utc::now().to_rfc3339(), "data": info }));
    };
    let sender_sum_stream = sender.clone();
    let sid_sum_stream = session_id.clone();
    let on_sum_stream = move |chunk: Value| {
        if abort_registry::is_aborted(&sid_sum_stream) { return; }
        sender_sum_stream.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_STREAM, "timestamp": chrono::Utc::now().to_rfc3339(), "data": chunk }));
    };
    let sender_sum_end = sender.clone();
    let sid_sum_end = session_id.clone();
    let on_sum_end = move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_end) { return; }
        sender_sum_end.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_END, "timestamp": chrono::Utc::now().to_rfc3339(), "data": info }));
    };

    let attachments_list = req.attachments.unwrap_or_default();
    let att = attachments::parse_attachments(&attachments_list);

    let callbacks = AiClientCallbacks {
        on_chunk: Some(std::sync::Arc::new(on_chunk)),
        on_thinking: Some(std::sync::Arc::new(on_thinking)),
        on_tools_start: Some(std::sync::Arc::new(on_tools_start)),
        on_tools_stream: Some(std::sync::Arc::new(on_tools_stream)),
        on_tools_end: Some(std::sync::Arc::new(on_tools_end)),
        on_context_summarized_start: Some(std::sync::Arc::new(on_sum_start)),
        on_context_summarized_stream: Some(std::sync::Arc::new(on_sum_stream)),
        on_context_summarized_end: Some(std::sync::Arc::new(on_sum_end)),
    };

    let result = run_chat(
        &session_id,
        &content,
        &model_cfg,
        req.user_id.clone(),
        att,
        req.reasoning_enabled,
        callbacks
    ).await;

    match result {
        Ok(res) => {
            if abort_registry::is_aborted(&session_id) {
                sender.send_json(&json!({ "type": Events::CANCELLED, "timestamp": chrono::Utc::now().to_rfc3339() }));
            } else {
                if !chunk_sent.load(Ordering::Relaxed) {
                    if let Some(text) = res.get("content").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            sender.send_json(&json!({ "type": Events::CHUNK, "timestamp": chrono::Utc::now().to_rfc3339(), "content": text }));
                        }
                    }
                }
                sender.send_json(&json!({ "type": Events::COMPLETE, "timestamp": chrono::Utc::now().to_rfc3339(), "result": res }));
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

fn agent_value(agent: &Agent, app_ids: Option<Vec<String>>) -> Value {
    let mut obj = json!({
        "id": agent.id.clone(),
        "name": agent.name.clone(),
        "ai_model_config_id": agent.ai_model_config_id.clone(),
        "system_context_id": agent.system_context_id.clone(),
        "description": agent.description.clone(),
        "user_id": agent.user_id.clone(),
        "mcp_config_ids": agent.mcp_config_ids.clone(),
        "callable_agent_ids": agent.callable_agent_ids.clone(),
        "project_id": agent.project_id.clone(),
        "workspace_dir": normalize_workspace_dir(agent.workspace_dir.as_deref()),
        "enabled": agent.enabled,
        "created_at": agent.created_at.clone(),
        "updated_at": agent.updated_at.clone()
    });
    if let Some(ids) = app_ids {
        if let Some(map) = obj.as_object_mut() {
            map.insert("app_ids".to_string(), json!(ids));
        }
    }
    obj
}

fn parse_id_list(raw: &Option<Value>) -> Option<Vec<String>> {
    let Some(val) = raw else { return None; };
    match val {
        Value::Array(arr) => {
            let list = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>();
            Some(list)
        }
        Value::String(s) => {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                if let Some(arr) = v.as_array() {
                    let list = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>();
                    return Some(list);
                }
            }
            Some(Vec::new())
        }
        _ => Some(Vec::new()),
    }
}

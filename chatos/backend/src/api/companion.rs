// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::modules::conversation_runtime::messages;
use crate::services::runtime_guidance_manager::runtime_guidance_manager;

use super::message_task_runner::list_companion_message_tasks;
use super::sessions::{get_session_compact_history, CompactHistoryQuery};

pub fn router() -> Router {
    Router::new()
        .route("/api/companion/conversations/{id}", get(get_conversation))
        .route(
            "/api/companion/conversations/{id}/compact-history",
            get(get_compact_history),
        )
        .route(
            "/api/companion/conversations/{id}/state",
            get(get_conversation_state),
        )
        .route("/api/companion/messages/{id}/tasks", get(get_message_tasks))
}

#[derive(Debug, Default, Deserialize)]
struct CompanionTaskQuery {
    task_id: Option<String>,
}

async fn get_message_tasks(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<CompanionTaskQuery>,
) -> (StatusCode, Json<Value>) {
    match list_companion_message_tasks(&auth, id.as_str()).await {
        Ok(tasks) => {
            let task_id = query
                .task_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            (
                StatusCode::OK,
                Json(json!({
                    "items": tasks
                        .iter()
                        .filter(|task| task_id.is_none_or(|id| {
                            task.get("id").and_then(Value::as_str) == Some(id)
                        }))
                        .filter_map(sanitize_task_item)
                        .collect::<Vec<_>>()
                })),
            )
        }
        Err(error) => error,
    }
}

async fn get_conversation(auth: AuthUser, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match ensure_owned_session(id.as_str(), &auth).await {
        Ok(session) => (
            StatusCode::OK,
            Json(json!({
                "id": session.id,
                "title": session.title,
                "status": session.status,
                "message_count": session.message_count,
                "updated_at": session.updated_at,
            })),
        ),
        Err(error) => map_session_access_error(error),
    }
}

async fn get_compact_history(
    auth: AuthUser,
    Path(id): Path<String>,
    query: Query<CompactHistoryQuery>,
) -> (StatusCode, Json<Value>) {
    let (status, Json(mut payload)) = get_session_compact_history(auth, Path(id), query).await;
    if status != StatusCode::OK {
        return (status, Json(payload));
    }
    if let Some(items) = payload.get_mut("items").and_then(Value::as_array_mut) {
        *items = items.drain(..).filter_map(sanitize_message_item).collect();
    }
    (status, Json(payload))
}

async fn get_conversation_state(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(error) = ensure_owned_session(id.as_str(), &auth).await {
        return map_session_access_error(error);
    }
    match messages::get_latest_turn_runtime_snapshot(id.as_str()).await {
        Ok(lookup) => {
            let turn_id = lookup.turn_id.filter(|value| !value.trim().is_empty());
            let active = turn_id.as_deref().is_some_and(|turn_id| {
                runtime_guidance_manager().is_active_turn(id.as_str(), turn_id)
            });
            (
                StatusCode::OK,
                Json(json!({
                    "turn_id": turn_id,
                    "status": lookup.status,
                    "active_in_runtime": active,
                })),
            )
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                json!({ "error": "Failed to load companion conversation state", "detail": error }),
            ),
        ),
    }
}

fn sanitize_message_item(value: Value) -> Option<Value> {
    let source = value.as_object()?;
    let role = source.get("role")?.as_str()?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }
    let mut safe = Map::new();
    for key in [
        "id",
        "revision",
        "sequence_no",
        "conversation_id",
        "conversationId",
        "role",
        "content",
        "message_mode",
        "message_source",
        "created_at",
    ] {
        if let Some(value) = source.get(key) {
            safe.insert(key.to_string(), value.clone());
        }
    }
    if source.get("message_mode").and_then(Value::as_str) == Some("task_runner_callback") {
        if let Some(task_id) = source
            .get("metadata")
            .and_then(Value::as_object)
            .and_then(|metadata| metadata.get("task_runner_async"))
            .and_then(Value::as_object)
            .and_then(|task| task.get("task_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            safe.insert("task_id".to_string(), Value::String(task_id.to_string()));
        }
    }
    Some(Value::Object(safe))
}

fn sanitize_task_item(value: &Value) -> Option<Value> {
    let source = value.as_object()?;
    let id = source.get("id")?.as_str()?;
    let title = source.get("title").and_then(Value::as_str).unwrap_or(id);
    let mut safe = Map::new();
    safe.insert("id".to_string(), Value::String(id.to_string()));
    safe.insert("title".to_string(), Value::String(title.to_string()));
    for key in [
        "description",
        "objective",
        "status",
        "priority",
        "tags",
        "result_summary",
        "process_log",
        "created_at",
        "updated_at",
    ] {
        if let Some(value) = source.get(key) {
            safe.insert(key.to_string(), value.clone());
        }
    }
    if let Some(last_run) = source.get("last_run").and_then(Value::as_object) {
        let mut safe_run = Map::new();
        for key in [
            "id",
            "status",
            "model_phase_status",
            "result_summary",
            "error_message",
            "started_at",
            "finished_at",
        ] {
            if let Some(value) = last_run.get(key) {
                safe_run.insert(key.to_string(), value.clone());
            }
        }
        if let Some(content) = last_run
            .get("report")
            .and_then(Value::as_object)
            .and_then(|report| report.get("content"))
            .and_then(Value::as_str)
        {
            safe_run.insert("report".to_string(), json!({ "content": content }));
        }
        safe.insert("last_run".to_string(), Value::Object(safe_run));
    }
    Some(Value::Object(safe))
}

#[cfg(test)]
mod tests {
    use super::{sanitize_message_item, sanitize_task_item};
    use serde_json::json;

    #[test]
    fn companion_history_removes_runtime_metadata_and_tool_records() {
        let safe = sanitize_message_item(json!({
            "id": "message-1",
            "role": "assistant",
            "content": "done",
            "metadata": { "workspace_root": "/private/work" },
            "reasoning": "private reasoning",
            "toolCalls": [{ "name": "shell" }]
        }))
        .expect("assistant message should remain visible");
        assert_eq!(safe["content"], "done");
        assert!(safe.get("metadata").is_none());
        assert!(safe.get("reasoning").is_none());
        assert!(safe.get("toolCalls").is_none());
        assert!(sanitize_message_item(json!({
            "id": "tool-1",
            "role": "tool",
            "content": "/private/work/secret"
        }))
        .is_none());
    }

    #[test]
    fn companion_history_marks_task_callback_without_exposing_metadata() {
        let safe = sanitize_message_item(json!({
            "id": "callback-1",
            "role": "assistant",
            "content": "任务已完成",
            "message_mode": "task_runner_callback",
            "metadata": {
                "workspace_root": "/private/work",
                "task_runner_async": {
                    "task_id": "task-1",
                    "run_id": "run-1",
                    "source_user_message_id": "message-1"
                }
            }
        }))
        .expect("task callback should remain visible");
        assert_eq!(safe["task_id"], "task-1");
        assert!(safe.get("metadata").is_none());
        assert!(safe.get("run_id").is_none());
        assert!(safe.get("source_user_message_id").is_none());
    }

    #[test]
    fn companion_task_detail_keeps_process_but_removes_execution_context() {
        let safe = sanitize_task_item(&json!({
            "id": "task-1",
            "title": "检查项目",
            "status": "running",
            "process_log": "[2026-09-15T10:00:00Z] 开始检查\n读取入口",
            "input_payload": { "root_path": "/private/project" },
            "mcp_config": { "secret": "hidden" },
            "execution_client_ref": "device-secret",
            "last_run": {
                "id": "run-1",
                "status": "running",
                "report": {
                    "content": "阶段结果",
                    "verification_evidence": ["/private/project/secret.txt"]
                },
                "internal_lease_id": "lease-secret"
            }
        }))
        .expect("valid task should remain visible");
        assert_eq!(
            safe["process_log"].as_str(),
            Some("[2026-09-15T10:00:00Z] 开始检查\n读取入口")
        );
        assert!(safe.get("input_payload").is_none());
        assert!(safe.get("mcp_config").is_none());
        assert!(safe.get("execution_client_ref").is_none());
        assert!(safe["last_run"].get("internal_lease_id").is_none());
        assert_eq!(safe["last_run"]["report"], json!({ "content": "阶段结果" }));
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::services::task_runner_api_client;

mod context;
mod graph;

use self::context::{
    resolve_message_task_runner_context, resolve_session_task_runner_context,
    task_matches_message_source, MessageTaskRunnerLookupQuery,
};
use self::graph::normalize_message_task_graph_payload_edges_with_tasks;

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/messages/:id/task-runner/tasks",
            get(list_message_task_runner_tasks),
        )
        .route(
            "/api/messages/:id/task-runner/graph",
            get(get_message_task_runner_graph),
        )
        .route(
            "/api/messages/:message_id/task-runner/tasks/:task_id",
            get(get_message_task_runner_task),
        )
        .route(
            "/api/messages/:message_id/task-runner/runs/:run_id",
            get(get_message_task_runner_run),
        )
        .route(
            "/api/messages/:message_id/task-runner/runs/:run_id/output/changes",
            get(get_message_task_runner_run_output_changes),
        )
        .route(
            "/api/messages/:message_id/task-runner/runs/:run_id/output/diff",
            get(get_message_task_runner_run_output_diff),
        )
        .route(
            "/api/messages/:message_id/task-runner/graph/runs/:run_id",
            get(get_message_task_runner_graph_run),
        )
        .route(
            "/api/conversations/:conversation_id/task-runner/active-message-tasks",
            post(get_conversation_task_runner_active_message_tasks),
        )
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ConversationTaskRunnerActiveMessageTasksRequest {
    source_user_message_ids: Option<Vec<String>>,
    source_turn_ids: Option<Vec<String>>,
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_text_items(values: Option<Vec<String>>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| normalize_text(Some(value.as_str())))
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn task_value_is_top_level(value: &Value) -> bool {
    normalize_text(value.get("parent_task_id").and_then(Value::as_str)).is_none()
}

async fn list_message_task_runner_tasks(
    auth: AuthUser,
    Path(message_id): Path<String>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(json!({
                    "items": [],
                    "source_session_id": null,
                    "source_user_message_id": null,
                    "source_turn_id": null,
                })),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::list_message_tasks(
        context.base_url.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取任务系统任务失败", "detail": err})),
            );
        }
    };
    let items = payload
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    task_value_is_top_level(item)
                        && task_matches_message_source(
                            item,
                            context.source_session_id.as_str(),
                            context.source_user_message_id.as_deref(),
                            context.source_turn_id.as_deref(),
                        )
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (
        StatusCode::OK,
        Json(json!({
            "items": items,
            "source_session_id": context.source_session_id,
            "source_user_message_id": context.source_user_message_id,
            "source_turn_id": context.source_turn_id,
        })),
    )
}

async fn get_conversation_task_runner_active_message_tasks(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(req): Json<ConversationTaskRunnerActiveMessageTasksRequest>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_session_task_runner_context(&auth, &conversation_id).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(json!({
                    "active_source_user_message_ids": [],
                    "running_source_user_message_ids": [],
                    "items": [],
                    "source_session_id": null,
                })),
            );
        }
        Err(err) => return err,
    };
    let source_user_message_ids = normalize_text_items(req.source_user_message_ids);
    let source_turn_ids = normalize_text_items(req.source_turn_ids);
    let payload = match task_runner_api_client::list_session_active_message_tasks(
        context.base_url.as_str(),
        context.source_session_id.as_str(),
        source_user_message_ids.as_slice(),
        source_turn_ids.as_slice(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取会话任务运行状态失败", "detail": err})),
            );
        }
    };
    (StatusCode::OK, Json(payload))
}

async fn get_message_task_runner_graph(
    auth: AuthUser,
    Path(message_id): Path<String>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(json!({
                    "root_task_ids": [],
                    "nodes": [],
                    "edges": [],
                    "source_session_id": null,
                    "source_user_message_id": null,
                    "source_turn_id": null,
                })),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::get_message_task_graph(
        context.base_url.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取任务流程图失败", "detail": err})),
            );
        }
    };
    let supplemental_tasks = match task_runner_api_client::list_message_tasks(
        context.base_url.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    )
    .await
    {
        Ok(tasks_payload) => tasks_payload
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|item| {
                        task_value_is_top_level(item)
                            && task_matches_message_source(
                                item,
                                context.source_session_id.as_str(),
                                context.source_user_message_id.as_deref(),
                                context.source_turn_id.as_deref(),
                            )
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        Err(err) => {
            warn!(
                error = err.as_str(),
                "message task graph normalization could not load supplemental task list"
            );
            Vec::new()
        }
    };
    (
        StatusCode::OK,
        Json(normalize_message_task_graph_payload_edges_with_tasks(
            payload,
            supplemental_tasks.as_slice(),
        )),
    )
}

async fn get_message_task_runner_task(
    auth: AuthUser,
    Path((message_id, task_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::get_message_task(
        context.base_url.as_str(),
        task_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取任务详情失败", "detail": err})),
            );
        }
    };
    if !task_matches_message_source(
        &payload,
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    ) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "任务不属于当前消息"})),
        );
    }
    (StatusCode::OK, Json(payload))
}

async fn get_message_task_runner_graph_run(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::get_message_graph_run(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
        query.event_limit(),
        query.event_offset(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取任务流程运行详情失败", "detail": err})),
            );
        }
    };
    (StatusCode::OK, Json(payload))
}

async fn get_message_task_runner_run(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::get_message_run(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
        query.event_limit(),
        query.event_offset(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取运行详情失败", "detail": err})),
            );
        }
    };
    let matches = payload.get("task").is_some_and(|task| {
        task_matches_message_source(
            task,
            context.source_session_id.as_str(),
            context.source_user_message_id.as_deref(),
            context.source_turn_id.as_deref(),
        )
    });
    if !matches {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "运行记录不属于当前消息"})),
        );
    }
    (StatusCode::OK, Json(payload))
}

async fn get_message_task_runner_run_output_changes(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::get_message_run_output_changes(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
        query.output_limit(),
        query.output_offset(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取运行输出变更失败", "detail": err})),
            );
        }
    };
    (StatusCode::OK, Json(payload))
}

async fn get_message_task_runner_run_output_diff(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let Some(diff_path) = query.output_path() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "path is required"})),
        );
    };
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::get_message_run_output_diff(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
        diff_path.as_str(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取运行输出 diff 失败", "detail": err})),
            );
        }
    };
    (StatusCode::OK, Json(payload))
}

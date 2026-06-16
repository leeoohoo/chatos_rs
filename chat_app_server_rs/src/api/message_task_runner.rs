use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::chat_runtime::ChatRuntimeMetadata;
use crate::core::messages::message_turn_id;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::models::message::Message;
use crate::models::session::Session;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{chatos_memory_mappings, task_runner_api_client};

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
            "/api/messages/:message_id/task-runner/graph/runs/:run_id",
            get(get_message_task_runner_graph_run),
        )
}

#[derive(Debug, Clone)]
struct MessageTaskRunnerContext {
    base_url: String,
    source_session_id: String,
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MessageTaskRunnerLookupQuery {
    session_id: Option<String>,
    conversation_id: Option<String>,
    source_session_id: Option<String>,
    turn_id: Option<String>,
    conversation_turn_id: Option<String>,
    source_turn_id: Option<String>,
    source_user_message_id: Option<String>,
}

impl MessageTaskRunnerLookupQuery {
    fn session_hint(&self) -> Option<String> {
        normalize_text(self.session_id.as_deref())
            .or_else(|| normalize_text(self.conversation_id.as_deref()))
            .or_else(|| normalize_text(self.source_session_id.as_deref()))
    }

    fn turn_hint(&self) -> Option<String> {
        normalize_text(self.turn_id.as_deref())
            .or_else(|| normalize_text(self.conversation_turn_id.as_deref()))
            .or_else(|| normalize_text(self.source_turn_id.as_deref()))
    }

    fn source_user_message_hint(&self) -> Option<String> {
        normalize_text(self.source_user_message_id.as_deref())
            .filter(|value| !is_temporary_message_id(value))
    }

    fn has_fallback_hints(&self) -> bool {
        self.session_hint().is_some()
            && (self.turn_hint().is_some() || self.source_user_message_hint().is_some())
    }
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn is_temporary_message_id(value: &str) -> bool {
    value.trim().starts_with("temp_")
}

fn metadata_string_at(metadata: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalize_text(cursor.as_str())
}

fn source_user_message_id_for_message(message: &crate::models::message::Message) -> Option<String> {
    if message.role.trim().eq_ignore_ascii_case("user") {
        return normalize_text(Some(message.id.as_str()));
    }
    metadata_string_at(
        message.metadata.as_ref(),
        &["task_runner_async", "source_user_message_id"],
    )
    .or_else(|| metadata_string_at(message.metadata.as_ref(), &["historyFinalForUserMessageId"]))
}

fn source_turn_id_for_message(message: &crate::models::message::Message) -> Option<String> {
    message_turn_id(message)
        .and_then(|value| normalize_text(Some(value)))
        .or_else(|| {
            metadata_string_at(
                message.metadata.as_ref(),
                &["task_runner_async", "source_turn_id"],
            )
        })
        .or_else(|| metadata_string_at(message.metadata.as_ref(), &["historyFinalForTurnId"]))
}

async fn find_message_in_session_by_lookup(
    session: &Session,
    query: &MessageTaskRunnerLookupQuery,
) -> Result<Option<Message>, String> {
    if let Some(source_user_message_id) = query.source_user_message_hint() {
        if let Some(message) =
            conversation_messages::get_message_by_id_in_session(session, &source_user_message_id)
                .await?
        {
            return Ok(Some(message));
        }
    }

    let Some(turn_id) = query.turn_hint() else {
        return Ok(None);
    };
    let messages = conversation_messages::list_messages(session.id.as_str(), None, 0, true).await?;
    Ok(messages.into_iter().find(|message| {
        message.role.trim().eq_ignore_ascii_case("user")
            && (message_turn_id(message) == Some(turn_id.as_str())
                || metadata_string_at(
                    message.metadata.as_ref(),
                    &["task_runner_async", "source_turn_id"],
                )
                .as_deref()
                    == Some(turn_id.as_str()))
    }))
}

fn task_matches_message_source(
    value: &Value,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> bool {
    if value
        .get("source_session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        != Some(source_session_id)
    {
        return false;
    }
    if let Some(source_user_message_id) = source_user_message_id {
        return value
            .get("source_user_message_id")
            .and_then(Value::as_str)
            .map(str::trim)
            == Some(source_user_message_id);
    }
    source_turn_id.is_some_and(|source_turn_id| {
        value
            .get("source_turn_id")
            .and_then(Value::as_str)
            .map(str::trim)
            == Some(source_turn_id)
    })
}

#[derive(Debug, Clone)]
struct GraphNodeEdgeSource {
    id: String,
    prerequisite_task_ids: Vec<String>,
}

fn graph_task_id(node: &Value) -> Option<String> {
    normalize_text(node.get("task")?.get("id")?.as_str())
}

fn task_id(task: &Value) -> Option<String> {
    normalize_text(task.get("id")?.as_str())
}

fn task_prerequisite_ids(task: &Value) -> Vec<String> {
    task.get("prerequisite_task_ids")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| normalize_text(item.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn graph_task_prerequisite_ids(node: &Value) -> Vec<String> {
    node.get("task")
        .map(task_prerequisite_ids)
        .unwrap_or_default()
}

fn task_prerequisite_summaries(task: &Value) -> Vec<Value> {
    task.get("prerequisite_tasks")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| task_id(item).is_some())
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn short_task_id(value: &str) -> String {
    if value.chars().count() > 16 {
        let prefix = value.chars().take(8).collect::<String>();
        let suffix = value
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        format!("{prefix}...{suffix}")
    } else {
        value.to_string()
    }
}

fn normalize_graph_task_shape(mut task: Value, task_id: &str, child_task: Option<&Value>) -> Value {
    if !task.is_object() {
        task = json!({});
    }
    let child_source_session_id =
        child_task.and_then(|task| normalize_text(task.get("source_session_id")?.as_str()));
    let child_source_turn_id =
        child_task.and_then(|task| normalize_text(task.get("source_turn_id")?.as_str()));
    let child_source_user_message_id =
        child_task.and_then(|task| normalize_text(task.get("source_user_message_id")?.as_str()));

    let Some(task_object) = task.as_object_mut() else {
        return json!({
            "id": task_id,
            "title": format!("前置任务 {}", short_task_id(task_id)),
            "prerequisite_task_ids": [],
            "prerequisite_tasks": [],
        });
    };
    task_object.insert("id".to_string(), json!(task_id));
    if normalize_text(task_object.get("title").and_then(Value::as_str)).is_none() {
        task_object.insert(
            "title".to_string(),
            json!(format!("前置任务 {}", short_task_id(task_id))),
        );
    }
    if !task_object
        .get("prerequisite_task_ids")
        .is_some_and(Value::is_array)
    {
        task_object.insert("prerequisite_task_ids".to_string(), json!([]));
    }
    if !task_object
        .get("prerequisite_tasks")
        .is_some_and(Value::is_array)
    {
        task_object.insert("prerequisite_tasks".to_string(), json!([]));
    }
    if normalize_text(task_object.get("source_session_id").and_then(Value::as_str)).is_none() {
        if let Some(value) = child_source_session_id {
            task_object.insert("source_session_id".to_string(), json!(value));
        }
    }
    if normalize_text(task_object.get("source_turn_id").and_then(Value::as_str)).is_none() {
        if let Some(value) = child_source_turn_id {
            task_object.insert("source_turn_id".to_string(), json!(value));
        }
    }
    if normalize_text(
        task_object
            .get("source_user_message_id")
            .and_then(Value::as_str),
    )
    .is_none()
    {
        if let Some(value) = child_source_user_message_id {
            task_object.insert("source_user_message_id".to_string(), json!(value));
        }
    }
    task
}

fn supplement_missing_graph_prerequisite_nodes(
    nodes: &mut Vec<Value>,
    supplemental_tasks: &[Value],
) {
    let supplemental_task_by_id = supplemental_tasks
        .iter()
        .filter_map(|task| task_id(task).map(|id| (id, task.clone())))
        .collect::<HashMap<_, _>>();
    let mut known_node_ids = nodes
        .iter()
        .filter_map(graph_task_id)
        .collect::<HashSet<_>>();
    let mut summary_by_id = HashMap::<String, Value>::new();

    let mut index = 0;
    while index < nodes.len() {
        let Some(child_task) = nodes.get(index).and_then(|node| node.get("task")).cloned() else {
            index += 1;
            continue;
        };
        for summary in task_prerequisite_summaries(&child_task) {
            if let Some(summary_id) = task_id(&summary) {
                summary_by_id.entry(summary_id).or_insert(summary);
            }
        }

        for prerequisite_task_id in task_prerequisite_ids(&child_task) {
            if known_node_ids.contains(prerequisite_task_id.as_str()) {
                continue;
            }
            let task = supplemental_task_by_id
                .get(prerequisite_task_id.as_str())
                .cloned()
                .or_else(|| summary_by_id.get(prerequisite_task_id.as_str()).cloned())
                .unwrap_or_else(|| json!({ "id": prerequisite_task_id }));
            let normalized_task =
                normalize_graph_task_shape(task, prerequisite_task_id.as_str(), Some(&child_task));
            nodes.push(json!({
                "depth": 0,
                "is_root": false,
                "is_current_message": false,
                "task": normalized_task,
            }));
            known_node_ids.insert(prerequisite_task_id);
        }
        index += 1;
    }
}

fn push_normalized_graph_edge(
    edge_ids: &mut HashSet<String>,
    normalized_edge_sources: &mut Vec<(String, String, String)>,
    known_node_ids: &HashSet<String>,
    source: Option<&str>,
    target: Option<&str>,
    kind: Option<&str>,
) {
    let Some(source) = normalize_text(source) else {
        return;
    };
    let Some(target) = normalize_text(target) else {
        return;
    };
    if source == target {
        return;
    }
    if !known_node_ids.contains(source.as_str()) || !known_node_ids.contains(target.as_str()) {
        return;
    }
    let edge_id = format!("{source}->{target}");
    if !edge_ids.insert(edge_id) {
        return;
    }
    normalized_edge_sources.push((
        source,
        target,
        normalize_text(kind).unwrap_or_else(|| "prerequisite".to_string()),
    ));
}

fn normalize_message_task_graph_payload_edges(payload: Value) -> Value {
    normalize_message_task_graph_payload_edges_with_tasks(payload, &[])
}

fn normalize_message_task_graph_payload_edges_with_tasks(
    mut payload: Value,
    supplemental_tasks: &[Value],
) -> Value {
    let Some(raw_nodes) = payload.get("nodes").and_then(Value::as_array) else {
        return payload;
    };
    let mut nodes = raw_nodes.clone();
    supplement_missing_graph_prerequisite_nodes(&mut nodes, supplemental_tasks);
    let node_sources = nodes
        .iter()
        .filter_map(|node| {
            graph_task_id(node).map(|id| GraphNodeEdgeSource {
                id,
                prerequisite_task_ids: graph_task_prerequisite_ids(node),
            })
        })
        .collect::<Vec<_>>();
    let mut edge_ids = HashSet::<String>::new();
    let mut normalized_edge_sources = Vec::<(String, String, String)>::new();
    let known_node_ids = node_sources
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();

    for node in &node_sources {
        for prerequisite_task_id in &node.prerequisite_task_ids {
            push_normalized_graph_edge(
                &mut edge_ids,
                &mut normalized_edge_sources,
                &known_node_ids,
                Some(prerequisite_task_id.as_str()),
                Some(node.id.as_str()),
                Some("prerequisite"),
            );
        }
    }
    if normalized_edge_sources.is_empty() {
        if let Some(edges) = payload.get("edges").and_then(Value::as_array) {
            for edge in edges {
                push_normalized_graph_edge(
                    &mut edge_ids,
                    &mut normalized_edge_sources,
                    &known_node_ids,
                    edge.get("source").and_then(Value::as_str),
                    edge.get("target").and_then(Value::as_str),
                    edge.get("kind").and_then(Value::as_str),
                );
            }
        }
    }

    let mut depth_by_id = known_node_ids
        .iter()
        .map(|task_id| (task_id.clone(), 0_i64))
        .collect::<HashMap<_, _>>();
    for _ in 0..known_node_ids.len() {
        let mut changed = false;
        for (source, target, _) in &normalized_edge_sources {
            let target_depth = depth_by_id.get(target.as_str()).copied().unwrap_or(0);
            let source_depth = depth_by_id.get(source.as_str()).copied().unwrap_or(0);
            let next_source_depth = target_depth + 1;
            if next_source_depth > source_depth {
                depth_by_id.insert(source.clone(), next_source_depth);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let normalized_edges = normalized_edge_sources
        .into_iter()
        .map(|(source, target, kind)| {
            json!({
                "id": format!("{source}->{target}"),
                "source": source,
                "target": target,
                "kind": kind,
            })
        })
        .collect::<Vec<_>>();
    for node in &mut nodes {
        let Some(task_id) = graph_task_id(node) else {
            continue;
        };
        let Some(depth) = depth_by_id.get(task_id.as_str()).copied() else {
            continue;
        };
        if let Some(node_object) = node.as_object_mut() {
            node_object.insert("depth".to_string(), json!(depth));
        }
    }
    if let Some(payload_object) = payload.as_object_mut() {
        payload_object.insert("nodes".to_string(), Value::Array(nodes));
        payload_object.insert("edges".to_string(), Value::Array(normalized_edges));
    }
    payload
}

async fn resolve_message_task_runner_context(
    auth: &AuthUser,
    message_id: &str,
    query: &MessageTaskRunnerLookupQuery,
) -> Result<Option<MessageTaskRunnerContext>, (StatusCode, Json<Value>)> {
    let direct_message = if is_temporary_message_id(message_id) && query.has_fallback_hints() {
        None
    } else {
        match conversation_messages::get_message_by_id(message_id).await {
            Ok(Some(message)) => Some(message),
            Ok(None) => None,
            Err(err) => {
                if !query.has_fallback_hints() {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": err})),
                    ));
                }
                warn!(
                    message_id,
                    error = err.as_str(),
                    "message task lookup by id failed; trying session/turn fallback"
                );
                None
            }
        }
    };

    let (session, message, source_user_message_id, source_turn_id) =
        if let Some(message) = direct_message {
            let session = match ensure_owned_session(&message.session_id, auth).await {
                Ok(session) => session,
                Err(err) => return Err(map_session_access_error(err)),
            };
            let source_user_message_id = source_user_message_id_for_message(&message);
            let source_turn_id = source_turn_id_for_message(&message).or_else(|| query.turn_hint());
            if source_user_message_id.is_none() && source_turn_id.is_none() {
                return Ok(None);
            }
            (
                session,
                Some(message),
                source_user_message_id,
                source_turn_id,
            )
        } else {
            let Some(session_id) = query.session_hint() else {
                return Err((StatusCode::NOT_FOUND, Json(json!({"error": "消息不存在"}))));
            };
            let session = match ensure_owned_session(session_id.as_str(), auth).await {
                Ok(session) => session,
                Err(err) => return Err(map_session_access_error(err)),
            };
            let message = match find_message_in_session_by_lookup(&session, query).await {
                Ok(message) => message,
                Err(err) => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "按会话轮次查找消息失败", "detail": err})),
                    ));
                }
            };
            let source_user_message_id = message
                .as_ref()
                .and_then(source_user_message_id_for_message)
                .or_else(|| query.source_user_message_hint());
            let source_turn_id = message
                .as_ref()
                .and_then(source_turn_id_for_message)
                .or_else(|| query.turn_hint());
            if source_user_message_id.is_none() && source_turn_id.is_none() {
                return Ok(None);
            }
            (session, message, source_user_message_id, source_turn_id)
        };

    let session_runtime = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    let message_runtime = ChatRuntimeMetadata::from_metadata(
        message.as_ref().and_then(|item| item.metadata.as_ref()),
    );
    let contact_id = session_runtime.contact_id.or(message_runtime.contact_id);
    let contact_agent_id = session_runtime
        .contact_agent_id
        .or_else(|| normalize_text(session.selected_agent_id.as_deref()))
        .or(message_runtime.contact_agent_id);
    let config = chatos_memory_mappings::get_contact_task_runner_runtime_config(
        Some(auth.user_id.as_str()),
        contact_id.as_deref(),
        contact_agent_id.as_deref(),
    )
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "读取联系人任务系统配置失败", "detail": err})),
        )
    })?;
    let Some(config) = config else {
        return Ok(None);
    };

    Ok(Some(MessageTaskRunnerContext {
        base_url: config.base_url,
        source_session_id: session.id,
        source_user_message_id,
        source_turn_id,
    }))
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
                    task_matches_message_source(
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
                        task_matches_message_source(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized_edges(payload: &Value) -> Vec<Value> {
        payload
            .get("edges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    fn node_depth(payload: &Value, task_id: &str) -> Option<i64> {
        payload
            .get("nodes")
            .and_then(Value::as_array)?
            .iter()
            .find(|node| graph_task_id(node).as_deref() == Some(task_id))?
            .get("depth")
            .and_then(Value::as_i64)
    }

    fn has_node(payload: &Value, task_id: &str) -> bool {
        payload
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(|nodes| {
                nodes
                    .iter()
                    .any(|node| graph_task_id(node).as_deref() == Some(task_id))
            })
    }

    #[test]
    fn normalize_graph_edges_keeps_parallel_prerequisites_parallel() {
        let payload = json!({
            "root_task_ids": ["current"],
            "nodes": [
                {
                    "depth": 0,
                    "is_root": true,
                    "is_current_message": true,
                    "task": {
                        "id": "current",
                        "title": "当前任务",
                        "prerequisite_task_ids": ["prereq-a", "prereq-b"]
                    }
                },
                {
                    "depth": 1,
                    "is_root": false,
                    "is_current_message": false,
                    "task": {
                        "id": "prereq-a",
                        "title": "前置 A",
                        "prerequisite_task_ids": []
                    }
                },
                {
                    "depth": 1,
                    "is_root": false,
                    "is_current_message": false,
                    "task": {
                        "id": "prereq-b",
                        "title": "前置 B",
                        "prerequisite_task_ids": []
                    }
                }
            ],
            "edges": [
                {
                    "id": "prereq-a->prereq-b",
                    "source": "prereq-a",
                    "target": "prereq-b",
                    "kind": "prerequisite"
                },
                {
                    "id": "prereq-b->current",
                    "source": "prereq-b",
                    "target": "current",
                    "kind": "prerequisite"
                }
            ]
        });

        let normalized = normalize_message_task_graph_payload_edges(payload);

        assert_eq!(
            normalized_edges(&normalized),
            vec![
                json!({
                    "id": "prereq-a->current",
                    "source": "prereq-a",
                    "target": "current",
                    "kind": "prerequisite"
                }),
                json!({
                    "id": "prereq-b->current",
                    "source": "prereq-b",
                    "target": "current",
                    "kind": "prerequisite"
                })
            ]
        );
        assert_eq!(node_depth(&normalized, "current"), Some(0));
        assert_eq!(node_depth(&normalized, "prereq-a"), Some(1));
        assert_eq!(node_depth(&normalized, "prereq-b"), Some(1));
    }

    #[test]
    fn normalize_graph_edges_keeps_declared_serial_prerequisite_edges() {
        let payload = json!({
            "root_task_ids": ["current"],
            "nodes": [
                {
                    "depth": 0,
                    "is_root": true,
                    "is_current_message": true,
                    "task": {
                        "id": "current",
                        "title": "当前任务",
                        "prerequisite_task_ids": ["prereq-a", "prereq-b"]
                    }
                },
                {
                    "depth": 1,
                    "is_root": false,
                    "is_current_message": false,
                    "task": {
                        "id": "prereq-a",
                        "title": "前置 A",
                        "prerequisite_task_ids": []
                    }
                },
                {
                    "depth": 1,
                    "is_root": false,
                    "is_current_message": false,
                    "task": {
                        "id": "prereq-b",
                        "title": "前置 B",
                        "prerequisite_task_ids": ["prereq-a"]
                    }
                }
            ],
            "edges": [
                {
                    "id": "prereq-b->current",
                    "source": "prereq-b",
                    "target": "current",
                    "kind": "prerequisite"
                }
            ]
        });

        let normalized = normalize_message_task_graph_payload_edges(payload);

        assert_eq!(
            normalized_edges(&normalized),
            vec![
                json!({
                    "id": "prereq-a->current",
                    "source": "prereq-a",
                    "target": "current",
                    "kind": "prerequisite"
                }),
                json!({
                    "id": "prereq-b->current",
                    "source": "prereq-b",
                    "target": "current",
                    "kind": "prerequisite"
                }),
                json!({
                    "id": "prereq-a->prereq-b",
                    "source": "prereq-a",
                    "target": "prereq-b",
                    "kind": "prerequisite"
                })
            ]
        );
        assert_eq!(node_depth(&normalized, "current"), Some(0));
        assert_eq!(node_depth(&normalized, "prereq-b"), Some(1));
        assert_eq!(node_depth(&normalized, "prereq-a"), Some(2));
    }

    #[test]
    fn normalize_graph_edges_adds_missing_prerequisite_nodes_from_task_list() {
        let payload = json!({
            "root_task_ids": ["current"],
            "nodes": [
                {
                    "depth": 0,
                    "is_root": true,
                    "is_current_message": true,
                    "task": {
                        "id": "current",
                        "title": "当前任务",
                        "source_session_id": "session-1",
                        "source_turn_id": "turn-1",
                        "source_user_message_id": "user-1",
                        "prerequisite_task_ids": ["prereq-a", "prereq-b"]
                    }
                },
                {
                    "depth": 1,
                    "is_root": false,
                    "is_current_message": false,
                    "task": {
                        "id": "prereq-b",
                        "title": "前置 B",
                        "source_session_id": "session-1",
                        "source_turn_id": "turn-1",
                        "source_user_message_id": "user-1",
                        "prerequisite_task_ids": []
                    }
                }
            ],
            "edges": [
                {
                    "id": "prereq-b->current",
                    "source": "prereq-b",
                    "target": "current",
                    "kind": "prerequisite"
                }
            ]
        });
        let supplemental_tasks = vec![json!({
            "id": "prereq-a",
            "title": "前置 A",
            "status": "succeeded",
            "source_session_id": "session-1",
            "source_turn_id": "turn-1",
            "source_user_message_id": "user-1",
            "prerequisite_task_ids": []
        })];

        let normalized =
            normalize_message_task_graph_payload_edges_with_tasks(payload, &supplemental_tasks);

        assert!(has_node(&normalized, "prereq-a"));
        assert_eq!(
            normalized_edges(&normalized),
            vec![
                json!({
                    "id": "prereq-a->current",
                    "source": "prereq-a",
                    "target": "current",
                    "kind": "prerequisite"
                }),
                json!({
                    "id": "prereq-b->current",
                    "source": "prereq-b",
                    "target": "current",
                    "kind": "prerequisite"
                })
            ]
        );
        assert_eq!(node_depth(&normalized, "current"), Some(0));
        assert_eq!(node_depth(&normalized, "prereq-a"), Some(1));
        assert_eq!(node_depth(&normalized, "prereq-b"), Some(1));
    }
}

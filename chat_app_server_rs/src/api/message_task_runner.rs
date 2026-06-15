use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
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
    (StatusCode::OK, Json(payload))
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

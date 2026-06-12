use axum::http::StatusCode;
use axum::{extract::Path, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::chat_runtime::ChatRuntimeMetadata;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{chatos_memory_mappings, task_runner_api_client};

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/messages/:id/task-runner/tasks",
            get(list_message_task_runner_tasks),
        )
        .route(
            "/api/messages/:message_id/task-runner/tasks/:task_id",
            get(get_message_task_runner_task),
        )
        .route(
            "/api/messages/:message_id/task-runner/runs/:run_id",
            get(get_message_task_runner_run),
        )
}

#[derive(Debug, Clone)]
struct MessageTaskRunnerContext {
    base_url: String,
    source_session_id: String,
    source_user_message_id: String,
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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

fn task_matches_message_source(
    value: &Value,
    source_session_id: &str,
    source_user_message_id: &str,
) -> bool {
    value
        .get("source_session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        == Some(source_session_id)
        && value
            .get("source_user_message_id")
            .and_then(Value::as_str)
            .map(str::trim)
            == Some(source_user_message_id)
}

async fn resolve_message_task_runner_context(
    auth: &AuthUser,
    message_id: &str,
) -> Result<Option<MessageTaskRunnerContext>, (StatusCode, Json<Value>)> {
    let message = match conversation_messages::get_message_by_id(message_id).await {
        Ok(Some(message)) => message,
        Ok(None) => {
            return Err((StatusCode::NOT_FOUND, Json(json!({"error": "消息不存在"}))));
        }
        Err(err) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            ));
        }
    };
    let session = match ensure_owned_session(&message.session_id, auth).await {
        Ok(session) => session,
        Err(err) => return Err(map_session_access_error(err)),
    };
    let Some(source_user_message_id) = source_user_message_id_for_message(&message) else {
        return Ok(None);
    };

    let session_runtime = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    let message_runtime = ChatRuntimeMetadata::from_metadata(message.metadata.as_ref());
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
        source_session_id: message.session_id,
        source_user_message_id,
    }))
}

async fn list_message_task_runner_tasks(
    auth: AuthUser,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(json!({
                    "items": [],
                    "source_session_id": null,
                    "source_user_message_id": null,
                })),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::list_message_tasks(
        context.base_url.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_str(),
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
                        context.source_user_message_id.as_str(),
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
        })),
    )
}

async fn get_message_task_runner_task(
    auth: AuthUser,
    Path((message_id, task_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id).await {
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
        context.source_user_message_id.as_str(),
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
        context.source_user_message_id.as_str(),
    ) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "任务不属于当前消息"})),
        );
    }
    (StatusCode::OK, Json(payload))
}

async fn get_message_task_runner_run(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id).await {
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
        context.source_user_message_id.as_str(),
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
            context.source_user_message_id.as_str(),
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

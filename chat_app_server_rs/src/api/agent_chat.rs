#[path = "agent_chat/tools_panel.rs"]
mod tools_panel;

use axum::http::StatusCode;
use axum::{
    extract::Path,
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};
use uuid::Uuid;

use self::tools_panel::{agent_status, agent_tools};
use crate::api::chat_stream_common::{validate_chat_stream_request, ChatStreamRequest};
use crate::api::conversation_semantics::extract_conversation_scope_id;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::chat_runtime::{metadata_string, ChatRuntimeMetadata};
use crate::core::messages::ensure_message_metadata_object;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::user_scope::ensure_and_set_user_id;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::modules::conversation_runtime::guidance;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::access_token_scope;
use crate::services::ai_common::normalize_turn_id;
use crate::services::chatos_sessions;
use crate::services::realtime::{publish_chat_stream_event, publish_sessions_updated};
use crate::utils::abort_registry;
use crate::utils::sse::SseSender;

pub fn router() -> Router {
    Router::new()
        .route("/api/agent/chat/send", post(agent_chat_send))
        .route("/api/agent/chat/stop", post(stop_chat))
        .route("/api/agent/tools", get(agent_tools))
        .route("/api/agent/status", get(agent_status))
        .route(
            "/api/agent/conversation/:conversation_id/reset",
            post(reset_conversation),
        )
}

pub fn public_router() -> Router {
    Router::new().route(
        "/api/agent/chat/task-runner/callback",
        post(task_runner_callback),
    )
}

#[derive(Debug, Deserialize)]
struct TaskRunnerCallbackRequest {
    event: String,
    task_id: String,
    run_id: Option<String>,
    status: String,
    task_title: String,
    result_summary: Option<String>,
    error_message: Option<String>,
    report_content: Option<String>,
    process_log: Option<String>,
    source_session_id: Option<String>,
    source_turn_id: Option<String>,
    source_user_message_id: Option<String>,
    parent_task_id: Option<String>,
    source_run_id: Option<String>,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
    schedule_mode: Option<String>,
    callback_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskRunnerCallbackResponse {
    accepted: bool,
    session_id: String,
    user_message_id: String,
    event: String,
}

async fn agent_chat_send(
    auth: AuthUser,
    Json(mut req): Json<ChatStreamRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    if let Err(err) = ensure_and_set_user_id(&mut req.user_id, &auth) {
        return Err(err);
    }
    validate_chat_stream_request(&req, false).await?;
    let conversation_id = req.conversation_id.clone().unwrap_or_default();
    let accepted_turn_id = normalize_turn_id(req.turn_id.as_deref());
    let user_message_id = Uuid::new_v4().to_string();
    req.user_message_id = Some(user_message_id.clone());

    abort_registry::reset_turn(&conversation_id, accepted_turn_id.as_deref());
    if let Some(turn_id) = accepted_turn_id.as_deref() {
        guidance::register_active_turn(&conversation_id, turn_id);
    }
    access_token_scope::spawn_with_current_access_token(stream_chat(None, req));

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": accepted_turn_id,
            "user_message_id": user_message_id,
            "source_user_message_id": user_message_id,
        })),
    ))
}

async fn reset_conversation(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    match conversation_messages::delete_messages_by_session(&conversation_id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "对话线程重置成功",
                "conversation_id": conversation_id
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "message": "重置对话线程失败",
                "detail": err,
                "conversation_id": conversation_id
            })),
        ),
    }
}

async fn stop_chat(Json(req): Json<Value>) -> (StatusCode, Json<Value>) {
    let conversation_id = extract_conversation_scope_id(&req).unwrap_or_default();
    let turn_id = normalize_turn_id(req.get("turn_id").and_then(Value::as_str));
    if conversation_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "message": "缺少 conversation_id"})),
        );
    }
    let ok = abort_registry::abort_turn(conversation_id.as_str(), turn_id.as_deref());
    if ok {
        return (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "停止中",
                "conversation_id": conversation_id,
                "turn_id": turn_id,
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "success": false,
            "message": if turn_id.is_some() {
                "当前轮次已切换，停止请求已忽略"
            } else {
                "未找到可停止的对话线程或已停止"
            },
            "conversation_id": conversation_id,
            "turn_id": turn_id,
        })),
    )
}

async fn stream_chat(sender: Option<SseSender>, req: ChatStreamRequest) {
    run_chat_usecase(RunChatUsecaseInput { sender, req }).await;
}

async fn task_runner_callback(
    headers: HeaderMap,
    Json(payload): Json<TaskRunnerCallbackRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = verify_task_runner_callback_secret(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "accepted": false, "error": err })),
        );
    }

    let Some(user_message_id) = normalize_callback_value(payload.source_user_message_id.as_deref())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "missing source_user_message_id" })),
        );
    };
    let Some(session_id) = normalize_callback_value(payload.source_session_id.as_deref()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "missing source_session_id" })),
        );
    };

    let session = match chatos_sessions::get_session_by_id(session_id.as_str()).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "accepted": false, "error": "session not found" })),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "accepted": false, "error": err })),
            );
        }
    };
    let mut user_message = match conversation_messages::get_message_by_id_in_session(
        &session,
        user_message_id.as_str(),
    )
    .await
    {
        Ok(Some(message)) => message,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "accepted": false, "error": "user message not found" })),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "accepted": false, "error": err })),
            );
        }
    };
    if user_message.session_id != session.id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "message session mismatch" })),
        );
    }

    let user_message_changed =
        apply_task_runner_callback_to_user_message(&mut user_message, &payload);
    let saved_user_message = if user_message_changed {
        match conversation_messages::upsert_message_in_session(&session, &user_message).await {
            Ok(message) => Some(message),
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "accepted": false, "error": err })),
                );
            }
        }
    } else {
        Some(user_message.clone())
    };

    let (saved_assistant_message, assistant_message_changed) = if is_task_runner_terminal_event(
        payload.event.as_str(),
    ) {
        let contact_display = build_task_runner_callback_contact_display(&session);
        let assistant_message = build_task_runner_callback_assistant_message_with_contact(
            &session.id,
            &payload,
            Some(&contact_display),
        );
        match conversation_messages::get_message_by_id_in_session(
            &session,
            assistant_message.id.as_str(),
        )
        .await
        {
            Ok(Some(existing_message)) if existing_message.session_id != session.id => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(
                        json!({ "accepted": false, "error": "assistant message session mismatch" }),
                    ),
                );
            }
            Ok(Some(existing_message))
                if messages_match_for_callback_upsert(&existing_message, &assistant_message) =>
            {
                (Some(existing_message), false)
            }
            Ok(_) => {
                match conversation_messages::upsert_message_in_session(&session, &assistant_message)
                    .await
                {
                    Ok(message) => (Some(message), true),
                    Err(err) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "accepted": false, "error": err })),
                        );
                    }
                }
            }
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "accepted": false, "error": err })),
                );
            }
        }
    } else {
        (None, false)
    };

    let session_changed = user_message_changed || assistant_message_changed;
    let refreshed_session = if session_changed {
        chatos_sessions::get_session_by_id(session.id.as_str())
            .await
            .ok()
            .flatten()
    } else {
        Some(session.clone())
    };

    let realtime_user_id = refreshed_session
        .as_ref()
        .and_then(|value| value.user_id.clone())
        .or_else(|| session.user_id.clone());
    let realtime_session_id = refreshed_session
        .as_ref()
        .map(|value| value.id.clone())
        .unwrap_or_else(|| session.id.clone());
    let realtime_project_id = refreshed_session
        .as_ref()
        .and_then(|value| value.project_id.clone())
        .or_else(|| session.project_id.clone());

    if let Some(user_id) = realtime_user_id.as_deref() {
        let callback_session = refreshed_session.as_ref().unwrap_or(&session);
        publish_task_runner_callback_realtime(
            user_id,
            callback_session,
            payload.source_turn_id.as_deref(),
            user_message_id.as_str(),
            payload.event.as_str(),
            saved_user_message.as_ref(),
            saved_assistant_message.as_ref(),
        );
        if session_changed {
            publish_sessions_updated(
                user_id,
                "task_runner_callback",
                Some(realtime_session_id.as_str()),
                realtime_project_id.as_deref(),
                refreshed_session,
            );
        }
    } else {
        warn!(
            session_id = realtime_session_id.as_str(),
            task_id = payload.task_id.as_str(),
            event = payload.event.as_str(),
            "task runner callback persisted without realtime user id; skipped realtime publish"
        );
    }

    info!(
        session_id = session_id.as_str(),
        user_message_id = user_message_id.as_str(),
        task_id = payload.task_id.as_str(),
        run_id = payload.run_id.as_deref().unwrap_or_default(),
        event = payload.event.as_str(),
        user_message_changed,
        assistant_message_changed,
        "accepted task runner callback"
    );

    (
        StatusCode::OK,
        Json(json!(TaskRunnerCallbackResponse {
            accepted: true,
            session_id,
            user_message_id,
            event: payload.event,
        })),
    )
}

fn verify_task_runner_callback_secret(headers: &HeaderMap) -> Result<(), String> {
    let expected = Config::try_get()
        .ok()
        .and_then(|config| config.task_runner_callback_secret.clone());
    let Some(expected) = expected.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let actual = headers
        .get("x-task-runner-callback-secret")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing task runner callback secret".to_string())?;
    if actual == expected {
        Ok(())
    } else {
        Err("invalid task runner callback secret".to_string())
    }
}

fn normalize_callback_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn apply_task_runner_callback_to_user_message(
    message: &mut Message,
    payload: &TaskRunnerCallbackRequest,
) -> bool {
    let original_metadata = message.metadata.clone();
    let source_user_message_id = message.id.clone();
    let metadata = ensure_message_metadata_object(message);
    let task_runner_meta = ensure_object_field(metadata, "task_runner_async");

    upsert_string(task_runner_meta, "mode", "contact_async");
    upsert_string(
        task_runner_meta,
        "source_user_message_id",
        source_user_message_id.as_str(),
    );
    if let Some(turn_id) = normalize_callback_value(payload.source_turn_id.as_deref()) {
        upsert_string(task_runner_meta, "source_turn_id", turn_id.as_str());
    }
    upsert_string(task_runner_meta, "last_event", payload.event.as_str());
    upsert_string(task_runner_meta, "last_task_id", payload.task_id.as_str());
    if let Some(run_id) = normalize_callback_value(payload.run_id.as_deref()) {
        upsert_string(task_runner_meta, "last_run_id", run_id.as_str());
    }
    if let Some(callback_at) = normalize_callback_value(payload.callback_at.as_deref()) {
        upsert_string(task_runner_meta, "last_event_at", callback_at.as_str());
    }

    let mut created_task_ids = read_string_set(task_runner_meta.get("created_task_ids"));
    let mut running_task_ids = read_string_set(task_runner_meta.get("running_task_ids"));
    let mut terminal_task_ids = read_string_set(task_runner_meta.get("terminal_task_ids"));
    let mut succeeded_task_ids = read_string_set(task_runner_meta.get("succeeded_task_ids"));
    let mut failed_task_ids = read_string_set(task_runner_meta.get("failed_task_ids"));
    let mut blocked_task_ids = read_string_set(task_runner_meta.get("blocked_task_ids"));
    let mut cancelled_task_ids = read_string_set(task_runner_meta.get("cancelled_task_ids"));
    let reset_task_terminal_state =
        |task_id: &str,
         terminal_task_ids: &mut std::collections::BTreeSet<String>,
         succeeded_task_ids: &mut std::collections::BTreeSet<String>,
         failed_task_ids: &mut std::collections::BTreeSet<String>,
         blocked_task_ids: &mut std::collections::BTreeSet<String>,
         cancelled_task_ids: &mut std::collections::BTreeSet<String>| {
            terminal_task_ids.remove(task_id);
            succeeded_task_ids.remove(task_id);
            failed_task_ids.remove(task_id);
            blocked_task_ids.remove(task_id);
            cancelled_task_ids.remove(task_id);
        };

    match payload.event.as_str() {
        "task.created" => {
            created_task_ids.insert(payload.task_id.clone());
        }
        "task.run.started" => {
            created_task_ids.insert(payload.task_id.clone());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            running_task_ids.insert(payload.task_id.clone());
        }
        "task.completed" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            succeeded_task_ids.insert(payload.task_id.clone());
            upsert_string(task_runner_meta, "overall_status", "completed");
        }
        "task.failed" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            failed_task_ids.insert(payload.task_id.clone());
            upsert_string(task_runner_meta, "overall_status", "completed");
        }
        "task.blocked" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            blocked_task_ids.insert(payload.task_id.clone());
            upsert_string(task_runner_meta, "overall_status", "completed");
        }
        "task.cancelled" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            cancelled_task_ids.insert(payload.task_id.clone());
            upsert_string(task_runner_meta, "overall_status", "completed");
        }
        _ => {}
    }

    write_string_set(task_runner_meta, "created_task_ids", &created_task_ids);
    write_string_set(task_runner_meta, "running_task_ids", &running_task_ids);
    write_string_set(task_runner_meta, "terminal_task_ids", &terminal_task_ids);
    write_string_set(task_runner_meta, "succeeded_task_ids", &succeeded_task_ids);
    write_string_set(task_runner_meta, "failed_task_ids", &failed_task_ids);
    write_string_set(task_runner_meta, "blocked_task_ids", &blocked_task_ids);
    write_string_set(task_runner_meta, "cancelled_task_ids", &cancelled_task_ids);

    message.metadata != original_metadata
}

fn messages_match_for_callback_upsert(existing: &Message, candidate: &Message) -> bool {
    existing.id == candidate.id
        && existing.session_id == candidate.session_id
        && existing.role == candidate.role
        && existing.content == candidate.content
        && existing.message_mode == candidate.message_mode
        && existing.message_source == candidate.message_source
        && existing.summary == candidate.summary
        && existing.tool_calls == candidate.tool_calls
        && existing.tool_call_id == candidate.tool_call_id
        && existing.reasoning == candidate.reasoning
        && existing.metadata == candidate.metadata
        && existing.summary_status == candidate.summary_status
        && existing.summary_id == candidate.summary_id
        && existing.summarized_at == candidate.summarized_at
        && existing.created_at == candidate.created_at
}

fn ensure_object_field<'a>(
    root: &'a mut serde_json::Map<String, Value>,
    key: &str,
) -> &'a mut serde_json::Map<String, Value> {
    let entry = root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(serde_json::Map::new());
    }
    match entry {
        Value::Object(map) => map,
        _ => unreachable!("entry must be object"),
    }
}

fn read_string_set(value: Option<&Value>) -> std::collections::BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn write_string_set(
    root: &mut serde_json::Map<String, Value>,
    key: &str,
    values: &std::collections::BTreeSet<String>,
) {
    root.insert(
        key.to_string(),
        Value::Array(values.iter().cloned().map(Value::String).collect()),
    );
}

fn upsert_string(root: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    root.insert(key.to_string(), Value::String(value.to_string()));
}

fn normalized_callback_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn preferred_callback_detail<'a>(
    payload: &'a TaskRunnerCallbackRequest,
) -> Option<(&'static str, &'static str, &'a str)> {
    if let Some(value) = normalized_callback_text(payload.result_summary.as_deref()) {
        return Some(("结果摘要", "result_summary", value));
    }
    if let Some(value) = normalized_callback_text(payload.report_content.as_deref()) {
        return Some(("关键输出", "report_content", value));
    }
    if let Some(value) = normalized_callback_text(payload.error_message.as_deref()) {
        return Some(("错误信息", "error_message", value));
    }
    None
}

fn is_task_runner_terminal_event(event: &str) -> bool {
    matches!(
        event,
        "task.completed" | "task.failed" | "task.blocked" | "task.cancelled"
    )
}

#[derive(Debug, Clone, Default)]
struct TaskRunnerCallbackContactDisplay {
    contact_id: Option<String>,
    contact_agent_id: Option<String>,
    display_name: Option<String>,
}

fn build_task_runner_callback_contact_display(
    session: &Session,
) -> TaskRunnerCallbackContactDisplay {
    let runtime = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    let display_name = metadata_string(
        session.metadata.as_ref(),
        &["contact", "agent_name_snapshot"],
    )
    .or_else(|| metadata_string(session.metadata.as_ref(), &["contact", "name"]))
    .or_else(|| {
        metadata_string(
            session.metadata.as_ref(),
            &["ui_contact", "agent_name_snapshot"],
        )
    })
    .or_else(|| metadata_string(session.metadata.as_ref(), &["ui_contact", "name"]))
    .or_else(|| metadata_string(session.metadata.as_ref(), &["chat_runtime", "contact_name"]))
    .or_else(|| normalize_callback_value(Some(session.title.as_str())));

    TaskRunnerCallbackContactDisplay {
        contact_id: runtime.contact_id,
        contact_agent_id: runtime.contact_agent_id,
        display_name,
    }
}

fn build_task_runner_callback_assistant_message(
    session_id: &str,
    payload: &TaskRunnerCallbackRequest,
) -> Message {
    build_task_runner_callback_assistant_message_with_contact(session_id, payload, None)
}

fn build_task_runner_callback_assistant_message_with_contact(
    session_id: &str,
    payload: &TaskRunnerCallbackRequest,
    contact_display: Option<&TaskRunnerCallbackContactDisplay>,
) -> Message {
    let mut message = Message::new(
        session_id.to_string(),
        "assistant".to_string(),
        build_task_runner_callback_message_content(payload),
    );
    message.id = build_task_runner_callback_message_id(payload);
    if let Some(callback_at) = normalize_callback_value(payload.callback_at.as_deref()) {
        message.created_at = callback_at;
    }
    message.message_mode = Some("task_runner_callback".to_string());
    message.message_source = Some("task_runner_service".to_string());
    let source_turn_id = normalize_callback_value(payload.source_turn_id.as_deref());
    let metadata = ensure_message_metadata_object(&mut message);
    if let Some(source_turn_id) = source_turn_id.as_deref() {
        upsert_string(metadata, "conversation_turn_id", source_turn_id);
    }
    let task_runner_meta = ensure_object_field(metadata, "task_runner_async");
    upsert_string(task_runner_meta, "mode", "contact_async");
    upsert_string(task_runner_meta, "message_kind", "task_terminal_update");
    if let Some(contact_display) = contact_display {
        if let Some(contact_id) = contact_display.contact_id.as_deref() {
            upsert_string(task_runner_meta, "contact_id", contact_id);
        }
        if let Some(contact_agent_id) = contact_display.contact_agent_id.as_deref() {
            upsert_string(task_runner_meta, "contact_agent_id", contact_agent_id);
        }
        if let Some(display_name) = contact_display.display_name.as_deref() {
            upsert_string(task_runner_meta, "contact_display_name", display_name);
            upsert_string(task_runner_meta, "agent_name_snapshot", display_name);
        }
    }
    upsert_string(task_runner_meta, "event", payload.event.as_str());
    upsert_string(task_runner_meta, "task_id", payload.task_id.as_str());
    upsert_string(task_runner_meta, "status", payload.status.as_str());
    upsert_string(task_runner_meta, "task_title", payload.task_title.as_str());
    if let Some(source_turn_id) = source_turn_id.as_deref() {
        upsert_string(task_runner_meta, "source_turn_id", source_turn_id);
    }
    if let Some(source_user_message_id) =
        normalize_callback_value(payload.source_user_message_id.as_deref())
    {
        upsert_string(
            task_runner_meta,
            "source_user_message_id",
            source_user_message_id.as_str(),
        );
    }
    if let Some(run_id) = normalize_callback_value(payload.run_id.as_deref()) {
        upsert_string(task_runner_meta, "run_id", run_id.as_str());
    }
    if let Some(parent_task_id) = normalize_callback_value(payload.parent_task_id.as_deref()) {
        upsert_string(task_runner_meta, "parent_task_id", parent_task_id.as_str());
    }
    if let Some(source_run_id) = normalize_callback_value(payload.source_run_id.as_deref()) {
        upsert_string(task_runner_meta, "source_run_id", source_run_id.as_str());
    }
    if let Some(schedule_mode) = normalize_callback_value(payload.schedule_mode.as_deref()) {
        upsert_string(task_runner_meta, "schedule_mode", schedule_mode.as_str());
    }
    if let Some(callback_at) = normalize_callback_value(payload.callback_at.as_deref()) {
        upsert_string(task_runner_meta, "callback_at", callback_at.as_str());
    }
    if let Some(result_summary) = normalized_callback_text(payload.result_summary.as_deref()) {
        upsert_string(task_runner_meta, "result_summary", result_summary);
    }
    if let Some(error_message) = normalized_callback_text(payload.error_message.as_deref()) {
        upsert_string(task_runner_meta, "error_message", error_message);
    }
    if let Some(report_content) = normalized_callback_text(payload.report_content.as_deref()) {
        upsert_string(task_runner_meta, "report_excerpt", report_content);
    }
    if let Some((_, detail_source, detail)) = preferred_callback_detail(payload) {
        upsert_string(task_runner_meta, "detail_source", detail_source);
        upsert_string(task_runner_meta, "detail_preview", detail);
    }
    message
}

fn build_task_runner_callback_message_id(payload: &TaskRunnerCallbackRequest) -> String {
    let source_user_message_id =
        normalize_callback_value(payload.source_user_message_id.as_deref())
            .unwrap_or_else(|| "unknown_user_message".to_string());
    let task_id = payload.task_id.trim();
    let event = payload.event.trim();
    let run_scope = normalize_callback_value(payload.run_id.as_deref())
        .or_else(|| normalize_callback_value(payload.source_run_id.as_deref()))
        .unwrap_or_else(|| payload.status.trim().to_string());
    format!("task_runner_callback::{source_user_message_id}::{task_id}::{event}::{run_scope}")
}

fn build_task_runner_callback_message_content(payload: &TaskRunnerCallbackRequest) -> String {
    let title = payload.task_title.trim();
    let headline = match payload.event.as_str() {
        "task.completed" => format!("任务「{}」已完成", title),
        "task.failed" => format!("任务「{}」执行失败", title),
        "task.blocked" => format!("任务「{}」当前被阻塞", title),
        "task.cancelled" => format!("任务「{}」已取消", title),
        _ => format!("任务「{}」状态更新", title),
    };
    match preferred_callback_detail(payload) {
        Some((label, _, detail)) => format!("{headline}\n\n{label}：\n{detail}"),
        None => headline,
    }
}

fn publish_task_runner_callback_realtime(
    user_id: &str,
    session: &crate::models::session::Session,
    turn_id: Option<&str>,
    user_message_id: &str,
    event: &str,
    user_message: Option<&Message>,
    assistant_message: Option<&Message>,
) {
    publish_chat_stream_event(
        user_id,
        session.id.as_str(),
        turn_id,
        session.project_id.as_deref(),
        Some(user_message_id),
        "chat.task_runner.updated",
        "task_runner_callback",
        json!({
            "type": "task_runner_callback",
            "event": event,
            "result": {
                "persisted_user_message": user_message,
                "persisted_user_message_id": user_message.map(|message| message.id.clone()),
                "persisted_assistant_message": assistant_message,
                "persisted_assistant_message_id": assistant_message.map(|message| message.id.clone()),
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        apply_task_runner_callback_to_user_message, build_task_runner_callback_assistant_message,
        build_task_runner_callback_message_id, is_task_runner_terminal_event,
        TaskRunnerCallbackRequest,
    };
    use crate::models::message::Message;

    fn sample_callback_payload() -> TaskRunnerCallbackRequest {
        TaskRunnerCallbackRequest {
            event: "task.completed".to_string(),
            task_id: "task-1".to_string(),
            run_id: Some("run-1".to_string()),
            status: "succeeded".to_string(),
            task_title: "Demo task".to_string(),
            result_summary: Some("done".to_string()),
            error_message: None,
            report_content: None,
            process_log: None,
            source_session_id: Some("session-1".to_string()),
            source_turn_id: Some("turn-1".to_string()),
            source_user_message_id: Some("user-1".to_string()),
            parent_task_id: None,
            source_run_id: None,
            prerequisite_task_ids: vec![],
            schedule_mode: Some("once".to_string()),
            callback_at: Some("2026-06-10T10:00:00Z".to_string()),
        }
    }

    #[test]
    fn callback_message_id_is_deterministic_for_same_run() {
        let payload = sample_callback_payload();
        let id = build_task_runner_callback_message_id(&payload);
        assert_eq!(
            id,
            "task_runner_callback::user-1::task-1::task.completed::run-1"
        );
    }

    #[test]
    fn callback_assistant_message_carries_idempotent_identity_and_async_metadata() {
        let payload = sample_callback_payload();
        let message = build_task_runner_callback_assistant_message("session-1", &payload);

        assert_eq!(
            message.id,
            "task_runner_callback::user-1::task-1::task.completed::run-1"
        );
        assert_eq!(message.created_at, "2026-06-10T10:00:00Z");
        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("mode"))
                .and_then(|value| value.as_str()),
            Some("contact_async")
        );
        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-1")
        );
        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("source_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-1")
        );
        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("callback_at"))
                .and_then(|value| value.as_str()),
            Some("2026-06-10T10:00:00Z")
        );
    }

    #[test]
    fn callback_updates_task_tracking_without_overwriting_existing_message_status() {
        let mut message = Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "please handle this".to_string(),
        );
        message.id = "user-1".to_string();
        message.metadata = Some(json!({
            "task_runner_async": {
                "overall_status": "completed"
            }
        }));

        let mut payload = sample_callback_payload();
        payload.event = "task.created".to_string();
        payload.task_id = "task-1".to_string();
        apply_task_runner_callback_to_user_message(&mut message, &payload);

        payload.event = "task.completed".to_string();
        apply_task_runner_callback_to_user_message(&mut message, &payload);

        let task_runner_async = message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .cloned()
            .unwrap_or_else(|| json!({}));

        assert_eq!(
            task_runner_async
                .get("overall_status")
                .and_then(|value| value.as_str()),
            Some("completed")
        );
        assert_eq!(
            task_runner_async
                .get("created_task_ids")
                .and_then(|value| value.as_array())
                .map(|value| value.len()),
            Some(1)
        );
        assert_eq!(
            task_runner_async
                .get("succeeded_task_ids")
                .and_then(|value| value.as_array())
                .map(|value| value.len()),
            Some(1)
        );
    }

    #[test]
    fn terminal_callback_marks_source_user_message_completed() {
        let mut message = Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "please handle this".to_string(),
        );
        message.id = "user-1".to_string();
        message.metadata = Some(json!({
            "task_runner_async": {
                "overall_status": "processing"
            }
        }));

        let payload = sample_callback_payload();
        apply_task_runner_callback_to_user_message(&mut message, &payload);

        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(|value| value.as_str()),
            Some("completed")
        );
    }

    #[test]
    fn task_runner_terminal_event_includes_failed_blocked_and_cancelled() {
        assert!(is_task_runner_terminal_event("task.completed"));
        assert!(is_task_runner_terminal_event("task.failed"));
        assert!(is_task_runner_terminal_event("task.blocked"));
        assert!(is_task_runner_terminal_event("task.cancelled"));
        assert!(!is_task_runner_terminal_event("task.created"));
    }

    #[test]
    fn failed_callback_assistant_message_keeps_error_detail() {
        let mut payload = sample_callback_payload();
        payload.event = "task.failed".to_string();
        payload.status = "failed".to_string();
        payload.result_summary = None;
        payload.error_message = Some("memory batch sync failed".to_string());

        let message = build_task_runner_callback_assistant_message("session-1", &payload);

        assert!(message.content.contains("任务「Demo task」执行失败"));
        assert!(message.content.contains("memory batch sync failed"));
        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("event"))
                .and_then(|value| value.as_str()),
            Some("task.failed")
        );
    }

    #[test]
    fn callback_message_content_keeps_full_detail() {
        let mut payload = sample_callback_payload();
        payload.result_summary = None;
        payload.report_content = Some("A".repeat(5_000));
        let message = build_task_runner_callback_assistant_message("session-1", &payload);
        let task_runner_async = message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .cloned()
            .unwrap_or_else(|| json!({}));

        assert!(message.content.chars().count() > 5_000);
        assert_eq!(
            task_runner_async
                .get("detail_source")
                .and_then(|value| value.as_str()),
            Some("report_content")
        );
        assert_eq!(
            task_runner_async
                .get("detail_preview")
                .and_then(|value| value.as_str())
                .map(|value| value.chars().count()),
            Some(5_000)
        );
    }
}

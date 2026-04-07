use chrono::{DateTime, Utc};
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::core::auth::AuthUser;
use crate::config::Config;
use crate::services::im_service_client::{
    self, ConversationMessageDto, CreateConversationMessageRequestDto,
    CreateConversationRequestDto, CreateConversationRunRequestDto, CreateImContactRequestDto,
    ImContactDto, ImConversationDto, UpdateConversationActionRequestDto, UpdateConversationRequestDto,
    UpdateConversationRunRequestDto,
};
use crate::services::im_orchestrator;
use crate::services::memory_server_client;
use crate::services::task_manager::{
    create_tasks_for_turn, submit_task_review_decision, TaskCreateReviewPayload, TaskDraft,
    TaskReviewAction, REVIEW_NOT_FOUND_ERR,
};
use crate::services::runtime_guidance_manager::{runtime_guidance_manager, EnqueueGuidanceError};
use crate::services::ui_prompt_manager::{
    get_ui_prompt_payload, get_ui_prompt_record_by_id, parse_response_submission,
    redact_response_for_store, submit_ui_prompt_response, update_ui_prompt_response,
    UiPromptPayload, UiPromptStatus, UI_PROMPT_NOT_FOUND_ERR, UI_PROMPT_TIMEOUT_MS_DEFAULT,
};
use crate::utils::abort_registry;

const ACTIVE_IM_RUN_STALE_SECS: i64 = 120;

pub fn router() -> Router {
    Router::new()
        .route("/api/im/contacts", get(list_contacts).post(create_contact))
        .route("/api/im/contacts/:contact_id", get(get_contact))
        .route("/api/im/ws-meta", get(get_im_ws_meta))
        .route(
            "/api/im/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/im/conversations/:conversation_id",
            get(get_conversation).patch(update_conversation),
        )
        .route(
            "/api/im/conversations/:conversation_id/read",
            axum::routing::post(mark_conversation_read),
        )
        .route(
            "/api/im/conversations/:conversation_id/messages",
            get(list_conversation_messages).post(create_conversation_message),
        )
        .route(
            "/api/im/conversations/:conversation_id/action-requests",
            get(list_action_requests),
        )
        .route(
            "/api/im/conversations/:conversation_id/runs",
            get(list_runs),
        )
        .route(
            "/api/im/action-requests/:action_request_id/submit",
            axum::routing::post(submit_action_request),
        )
}

#[derive(Debug, Deserialize)]
struct ListMessagesQuery {
    limit: Option<i64>,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubmitTaskReviewRequest {
    action: TaskReviewAction,
    tasks: Option<Vec<TaskDraft>>,
    reason: Option<String>,
}

async fn list_contacts(_auth: AuthUser) -> (StatusCode, Json<Value>) {
    match im_service_client::list_contacts().await {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => bad_gateway("IM 服务不可用", err),
    }
}

async fn get_im_ws_meta(_auth: AuthUser) -> (StatusCode, Json<Value>) {
    let base_url = Config::get().im_service_base_url.trim().trim_end_matches('/');
    let ws_base = if let Some(stripped) = base_url.strip_prefix("https://") {
        format!("wss://{}", stripped)
    } else if let Some(stripped) = base_url.strip_prefix("http://") {
        format!("ws://{}", stripped)
    } else {
        base_url.to_string()
    };
    let ws_url = if let Some(prefix) = ws_base.strip_suffix("/api/im/v1") {
        format!("{}/api/im/v1/ws", prefix)
    } else {
        format!("{}/ws", ws_base)
    };

    (StatusCode::OK, Json(json!({ "ws_url": ws_url })))
}

async fn create_contact(
    auth: AuthUser,
    Json(mut req): Json<CreateImContactRequestDto>,
) -> (StatusCode, Json<Value>) {
    req.owner_user_id = Some(auth.user_id);
    match im_service_client::create_contact(&req).await {
        Ok(item) => (StatusCode::CREATED, Json(json!(item))),
        Err(err) => map_im_error("创建联系人失败", err),
    }
}

async fn get_contact(_auth: AuthUser, Path(contact_id): Path<String>) -> (StatusCode, Json<Value>) {
    match im_service_client::get_contact(contact_id.as_str()).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => map_im_error("获取联系人失败", err),
    }
}

async fn list_conversations(_auth: AuthUser) -> (StatusCode, Json<Value>) {
    match im_service_client::list_conversations().await {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => bad_gateway("IM 服务不可用", err),
    }
}

async fn create_conversation(
    auth: AuthUser,
    Json(mut req): Json<CreateConversationRequestDto>,
) -> (StatusCode, Json<Value>) {
    req.owner_user_id = Some(auth.user_id);
    match im_service_client::create_conversation(&req).await {
        Ok(item) => (StatusCode::CREATED, Json(json!(item))),
        Err(err) => map_im_error("创建会话失败", err),
    }
}

async fn get_conversation(
    _auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match im_service_client::get_conversation(conversation_id.as_str()).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => map_im_error("获取会话失败", err),
    }
}

async fn update_conversation(
    _auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(req): Json<UpdateConversationRequestDto>,
) -> (StatusCode, Json<Value>) {
    match im_service_client::update_conversation(conversation_id.as_str(), &req).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => map_im_error("更新会话失败", err),
    }
}

async fn mark_conversation_read(
    _auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match im_service_client::mark_conversation_read(conversation_id.as_str()).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => map_im_error("会话已读失败", err),
    }
}

async fn list_conversation_messages(
    _auth: AuthUser,
    Path(conversation_id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    match im_service_client::list_conversation_messages(
        conversation_id.as_str(),
        query.limit,
        query.order.as_deref(),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => map_im_error("获取消息失败", err),
    }
}

async fn create_conversation_message(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(mut req): Json<CreateConversationMessageRequestDto>,
) -> (StatusCode, Json<Value>) {
    let is_user_message = req.sender_type.trim().eq_ignore_ascii_case("user");
    if is_user_message {
        req.sender_id = Some(auth.user_id);
        if req.delivery_status.is_none() {
            req.delivery_status = Some("sent".to_string());
        }
    }
    match im_service_client::create_conversation_message(conversation_id.as_str(), &req).await {
        Ok(item) => {
            if is_user_message {
                dispatch_contact_scope_input(conversation_id.as_str(), &item).await;
            }
            (StatusCode::CREATED, Json(json!(item)))
        }
        Err(err) => map_im_error("发送消息失败", err),
    }
}

async fn list_action_requests(
    _auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match im_service_client::list_action_requests(conversation_id.as_str()).await {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => map_im_error("获取待确认动作失败", err),
    }
}

async fn list_runs(_auth: AuthUser, Path(conversation_id): Path<String>) -> (StatusCode, Json<Value>) {
    match im_service_client::list_runs(conversation_id.as_str()).await {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => map_im_error("获取运行记录失败", err),
    }
}

async fn submit_action_request(
    _auth: AuthUser,
    Path(action_request_id): Path<String>,
    Json(raw): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let action_request = match im_service_client::get_action_request_internal(action_request_id.as_str()).await {
        Ok(item) => item,
        Err(err) => return map_im_error("获取动作请求失败", err),
    };
    let conversation = match im_service_client::get_conversation(action_request.conversation_id.as_str()).await {
        Ok(item) => item,
        Err(err) => return map_im_error("获取会话失败", err),
    };

    let (next_status, stored_submission, result_payload) = match action_request.action_type.as_str() {
        "task_review" => match submit_task_review_action(action_request.payload.clone(), raw.clone()).await {
            Ok(value) => value,
            Err(err) => return bad_request_json("提交任务确认失败", err),
        },
        "ui_prompt" => match submit_ui_prompt_action(action_request.payload.clone(), raw.clone()).await {
            Ok(value) => value,
            Err(err) => {
                if err == UI_PROMPT_NOT_FOUND_ERR {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({"success": false, "error": err})),
                    );
                }
                return bad_request_json("提交表单失败", err);
            }
        },
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"success": false, "error": format!("unsupported action_type: {}", other)})),
            )
        }
    };

    let updated = match im_service_client::update_action_request_internal(
        action_request_id.as_str(),
        &UpdateConversationActionRequestDto {
            status: Some(next_status.clone()),
            submitted_payload: Some(stored_submission),
        },
    )
    .await
    {
        Ok(item) => item,
        Err(err) => return map_im_error("更新动作请求失败", err),
    };

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "conversation_id": conversation.id,
            "action_request": updated,
            "result": result_payload,
        })),
    )
}

fn map_im_error(scene: &str, err: String) -> (StatusCode, Json<Value>) {
    if err.contains("status=401") || err.contains("status=403") {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "forbidden", "detail": err})),
        );
    }
    if err.contains("status=404") {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": scene, "detail": err})),
        );
    }
    if err.contains("status=400") {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": scene, "detail": err})),
        );
    }
    bad_gateway(scene, err)
}

fn bad_gateway(scene: &str, err: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({"error": scene, "detail": err})),
    )
}

fn bad_request_json(scene: &str, err: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"success": false, "error": scene, "detail": err})),
    )
}

async fn enqueue_conversation_run(
    conversation_id: &str,
    message: &im_service_client::ConversationMessageDto,
) {
    let conversation = match im_service_client::get_conversation(conversation_id).await {
        Ok(item) => item,
        Err(err) => {
            warn!(
                "[IM-ORCH] load conversation failed when enqueue run: conversation_id={} message_id={} error={}",
                conversation_id, message.id, err
            );
            return;
        }
    };
    let contact = match resolve_execution_contact(
        conversation.owner_user_id.as_str(),
        conversation.contact_id.as_str(),
    )
    .await
    {
        Ok(item) => item,
        Err(err) => {
            warn!(
                "[IM-ORCH] resolve execution contact failed when enqueue run: conversation_id={} contact_id={} message_id={} error={}",
                conversation_id, conversation.contact_id, message.id, err
            );
            return;
        }
    };

    let req = CreateConversationRunRequestDto {
        conversation_id: conversation.id.clone(),
        source_message_id: message.id.clone(),
        contact_id: contact.id.clone(),
        agent_id: contact.agent_id.clone(),
        project_id: conversation.project_id.clone(),
        execution_session_id: None,
        execution_turn_id: None,
        execution_scope_key: None,
        status: Some("queued".to_string()),
        started_at: None,
    };

    match im_service_client::create_run_internal(&req).await {
        Ok(run) => im_orchestrator::spawn_process_run(run, conversation, contact, message.clone()),
        Err(err) => {
            warn!(
                "[IM-ORCH] create queued run failed: conversation_id={} message_id={} contact_id={} agent_id={} error={}",
                conversation_id, message.id, contact.id, contact.agent_id, err
            );
        }
    }
}

async fn dispatch_contact_scope_input(
    conversation_id: &str,
    message: &ConversationMessageDto,
) {
    info!(
        "[IM-ORCH] dispatch input begin: conversation_id={} message_id={} sender_type={}",
        conversation_id, message.id, message.sender_type
    );
    let conversation = match im_service_client::get_conversation(conversation_id).await {
        Ok(item) => item,
        Err(err) => {
            warn!(
                "[IM-ORCH] load conversation failed when dispatch input: conversation_id={} message_id={} error={}",
                conversation_id, message.id, err
            );
            enqueue_conversation_run(conversation_id, message).await;
            return;
        }
    };
    let contact = match resolve_execution_contact(
        conversation.owner_user_id.as_str(),
        conversation.contact_id.as_str(),
    )
    .await
    {
        Ok(item) => item,
        Err(err) => {
            warn!(
                "[IM-ORCH] resolve execution contact failed when dispatch input: conversation_id={} message_id={} error={}",
                conversation_id, message.id, err
            );
            enqueue_conversation_run(conversation_id, message).await;
            return;
        }
    };

    match try_enqueue_runtime_guidance_for_scope(&conversation, &contact, message).await {
        Ok(true) => {}
        Ok(false) => {
            info!(
                "[IM-ORCH] no active runtime found, creating new run: conversation_id={} message_id={}",
                conversation_id, message.id
            );
            enqueue_conversation_run(conversation_id, message).await
        }
        Err(err) => {
            warn!(
                "[IM-ORCH] dispatch input failed, fallback to new run: conversation_id={} message_id={} error={}",
                conversation_id, message.id, err
            );
            enqueue_conversation_run(conversation_id, message).await;
        }
    }
}

async fn try_enqueue_runtime_guidance_for_scope(
    conversation: &ImConversationDto,
    contact: &ImContactDto,
    message: &ConversationMessageDto,
) -> Result<bool, String> {
    if try_enqueue_im_run_guidance(conversation, contact, message).await? {
        return Ok(true);
    }
    Ok(false)
}

async fn try_enqueue_im_run_guidance(
    conversation: &ImConversationDto,
    contact: &ImContactDto,
    message: &ConversationMessageDto,
) -> Result<bool, String> {
    let runs = im_service_client::list_runs(conversation.id.as_str()).await?;
    let active_run = runs.into_iter().find(|run| {
        let status = run.status.trim().to_ascii_lowercase();
        status == "running" || status == "queued"
    });
    let Some(run) = active_run else {
        return Ok(false);
    };
    if retire_stale_im_run_if_needed(conversation, &run).await? {
        return Ok(false);
    }
    let execution_session_id = run
        .execution_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let execution_turn_id = run
        .execution_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if execution_session_id.is_none() || execution_turn_id.is_none() {
        info!(
            "[IM-ORCH] queued run not ready yet, hold message for startup merge: conversation_id={} message_id={} run_id={}",
            conversation.id, message.id, run.id
        );
        create_guidance_ack_message(
            conversation,
            contact,
            message,
            "收到，我会把这条补充和上一条一起合并处理。",
            json!({
                "type": "runtime_guidance_ack",
                "guidance_target": "im_run_starting",
                "run_id": run.id,
                "status": run.status,
            }),
        )
        .await?;
        return Ok(true);
    }

    let guidance = match runtime_guidance_manager().enqueue_guidance(
        execution_session_id.unwrap_or_default(),
        execution_turn_id.unwrap_or_default(),
        message.content.as_str(),
    ) {
        Ok(item) => item,
        Err(EnqueueGuidanceError::TurnNotRunning) => return Ok(false),
    };

    info!(
        "[IM-ORCH] routed user message as im runtime guidance: conversation_id={} message_id={} run_id={} guidance_id={}",
        conversation.id, message.id, run.id, guidance.guidance_id
    );
    create_guidance_ack_message(
        conversation,
        contact,
        message,
        "收到，我会把这条补充合并进当前处理中内容。",
        json!({
            "type": "runtime_guidance_ack",
            "guidance_target": "im_run",
            "guidance_id": guidance.guidance_id,
            "run_id": run.id,
        }),
    )
    .await?;
    Ok(true)
}

async fn retire_stale_im_run_if_needed(
    conversation: &ImConversationDto,
    run: &im_service_client::ConversationRunDto,
) -> Result<bool, String> {
    let Some(age_secs) = active_run_age_secs(run) else {
        return Ok(false);
    };
    if age_secs < ACTIVE_IM_RUN_STALE_SECS {
        return Ok(false);
    }

    warn!(
        "[IM-ORCH] retire stale active run before accepting new message: conversation_id={} run_id={} status={} age_secs={}",
        conversation.id, run.id, run.status, age_secs
    );

    if let Some(session_id) = run
        .execution_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let _ = abort_registry::abort(session_id);
    }

    im_service_client::update_run_internal(
        run.id.as_str(),
        &UpdateConversationRunRequestDto {
            status: Some("failed".to_string()),
            error_message: Some(format!(
                "stale_active_run_timeout:{}s",
                age_secs
            )),
            execution_session_id: run.execution_session_id.clone(),
            execution_turn_id: run.execution_turn_id.clone(),
            execution_scope_key: run.execution_scope_key.clone(),
            started_at: run.started_at.clone(),
            finished_at: Some(crate::core::time::now_rfc3339()),
            ..UpdateConversationRunRequestDto::default()
        },
    )
    .await?;

    Ok(true)
}

fn active_run_age_secs(run: &im_service_client::ConversationRunDto) -> Option<i64> {
    let started_at = run
        .started_at
        .as_deref()
        .or(Some(run.created_at.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let parsed = DateTime::parse_from_rfc3339(started_at).ok()?;
    let now = Utc::now();
    Some((now - parsed.with_timezone(&Utc)).num_seconds().max(0))
}

async fn create_guidance_ack_message(
    conversation: &ImConversationDto,
    contact: &ImContactDto,
    source_message: &ConversationMessageDto,
    content: &str,
    metadata: Value,
) -> Result<ConversationMessageDto, String> {
    let mut metadata = metadata;
    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "reply_context".to_string(),
            json!({
                "message_id": source_message.id,
                "sender_type": source_message.sender_type,
                "preview": truncate_preview(source_message.content.as_str()),
            }),
        );
    }
    im_service_client::create_conversation_message_internal(
        conversation.id.as_str(),
        &CreateConversationMessageRequestDto {
            sender_type: "contact".to_string(),
            sender_id: Some(contact.id.clone()),
            message_type: Some("text".to_string()),
            content: content.to_string(),
            delivery_status: Some("sent".to_string()),
            client_message_id: None,
            reply_to_message_id: Some(source_message.id.clone()),
            metadata: Some(metadata),
        },
    )
    .await
}

fn truncate_preview(value: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let preview: String = trimmed.chars().take(MAX_CHARS).collect();
    if trimmed.chars().count() > MAX_CHARS {
        format!("{}...", preview)
    } else {
        preview
    }
}

async fn resolve_execution_contact(
    owner_user_id: &str,
    conversation_contact_id: &str,
) -> Result<im_service_client::ImContactDto, String> {
    let normalized_contact_id = conversation_contact_id.trim();
    if normalized_contact_id.is_empty() {
        return Err("conversation.contact_id is empty".to_string());
    }

    if let Ok(contact) = im_service_client::get_contact(normalized_contact_id).await {
        let same_owner = owner_user_id.trim().is_empty()
            || contact.owner_user_id.trim() == owner_user_id.trim();
        if same_owner {
            return Ok(contact);
        }
    }

    let memory_contacts =
        memory_server_client::list_memory_contacts(Some(owner_user_id), Some(2000), 0).await?;

    let matched_memory_contact = memory_contacts
        .iter()
        .find(|item| item.id.trim() == normalized_contact_id);
    let resolved_agent_id = matched_memory_contact
        .map(|item| item.agent_id.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            memory_contacts
                .iter()
                .find(|item| item.agent_id.trim() == normalized_contact_id)
                .map(|item| item.agent_id.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .ok_or_else(|| {
            format!(
                "unable to resolve execution contact from conversation.contact_id={}",
                normalized_contact_id
            )
        })?;

    if let Some(existing) = im_service_client::list_contacts()
        .await?
        .into_iter()
        .find(|item| {
            item.owner_user_id.trim() == owner_user_id.trim()
                && item.agent_id.trim() == resolved_agent_id.as_str()
        })
    {
        return Ok(existing);
    }

    let display_name = matched_memory_contact
        .and_then(|item| item.agent_name_snapshot.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| resolved_agent_id.clone());

    warn!(
        "[IM-ORCH] auto create IM contact for execution: owner_user_id={} conversation_contact_id={} agent_id={}",
        owner_user_id,
        normalized_contact_id,
        resolved_agent_id
    );

    im_service_client::create_contact(&CreateImContactRequestDto {
        owner_user_id: Some(owner_user_id.to_string()),
        agent_id: resolved_agent_id,
        display_name,
        avatar_url: None,
    })
    .await
}

async fn submit_task_review_action(
    payload: Value,
    raw: Value,
) -> Result<(String, Value, Value), String> {
    let payload = parse_action_request_payload(payload)?;
    let review_payload = serde_json::from_value::<TaskCreateReviewPayload>(payload.clone())
        .map_err(|err| format!("invalid task review action_request payload: {}", err))?;
    let review_id = payload
        .get("review_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "review_id is required".to_string())?;
    let req: SubmitTaskReviewRequest =
        serde_json::from_value(raw).map_err(|err| format!("invalid task review payload: {}", err))?;
    if matches!(req.action, TaskReviewAction::Confirm) {
        let empty = req.tasks.as_ref().map(|tasks| tasks.is_empty()).unwrap_or(true);
        if empty {
            return Err("tasks is required for confirm action".to_string());
        }
    }

    let result = match submit_task_review_decision(
        review_id,
        req.action,
        req.tasks.clone(),
        req.reason.clone(),
    )
    .await
    {
        Ok(result) => result,
        Err(err)
            if err == REVIEW_NOT_FOUND_ERR || err == "review_listener_closed" =>
        {
            warn!(
                review_id = review_id,
                action = req.action.as_str(),
                error = %err,
                "task review hub entry missing; fallback to persisted IM action request payload"
            );
            resolve_task_review_from_persisted_payload(&review_payload, &req).await?
        }
        Err(err) => return Err(err),
    };
    let stored_submission = json!({
        "action": req.action.as_str(),
        "tasks": req.tasks,
        "reason": req.reason,
    });
    Ok((
        match req.action {
            TaskReviewAction::Confirm => "confirmed".to_string(),
            TaskReviewAction::Cancel => "canceled".to_string(),
        },
        stored_submission,
        json!({
            "review_id": result.review_id,
            "session_id": result.session_id,
            "conversation_turn_id": result.conversation_turn_id,
            "action": req.action.as_str(),
        }),
    ))
}

async fn resolve_task_review_from_persisted_payload(
    review_payload: &TaskCreateReviewPayload,
    req: &SubmitTaskReviewRequest,
) -> Result<TaskCreateReviewPayload, String> {
    match req.action {
        TaskReviewAction::Confirm => {
            let tasks = req
                .tasks
                .clone()
                .unwrap_or_else(|| review_payload.draft_tasks.clone());
            let empty = tasks.is_empty();
            if empty {
                return Err("tasks is required for confirm action".to_string());
            }
            let _ = create_tasks_for_turn(
                review_payload.session_id.as_str(),
                review_payload.conversation_turn_id.as_str(),
                tasks,
            )
            .await?;
        }
        TaskReviewAction::Cancel => {}
    }

    Ok(review_payload.clone())
}

async fn submit_ui_prompt_action(
    payload: Value,
    raw: Value,
) -> Result<(String, Value, Value), String> {
    let payload = parse_action_request_payload(payload)?;
    let prompt_id = payload
        .get("prompt_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "prompt_id is required".to_string())?;

    let prompt_payload = load_ui_prompt_payload(prompt_id).await?;
    let submission = parse_response_submission(raw, &prompt_payload)?;
    let resolved = match submit_ui_prompt_response(prompt_id, submission.clone()).await {
        Ok(value) => Some(value),
        Err(err) if err == UI_PROMPT_NOT_FOUND_ERR || err == "ui_prompt_listener_closed" => None,
        Err(err) => return Err(err),
    };

    let status = UiPromptStatus::from_str(submission.status.as_str()).unwrap_or(UiPromptStatus::Canceled);
    let redacted_response = redact_response_for_store(&submission, &prompt_payload);
    let _ = update_ui_prompt_response(prompt_id, status, Some(redacted_response.clone())).await;

    Ok((
        status.as_str().to_string(),
        redacted_response,
        json!({
            "prompt_id": prompt_id,
            "session_id": resolved
                .as_ref()
                .map(|item| item.session_id.clone())
                .unwrap_or_else(|| prompt_payload.session_id.clone()),
            "conversation_turn_id": resolved
                .as_ref()
                .map(|item| item.conversation_turn_id.clone())
                .unwrap_or_else(|| prompt_payload.conversation_turn_id.clone()),
            "status": submission.status,
        }),
    ))
}

fn parse_action_request_payload(payload: Value) -> Result<Value, String> {
    match payload {
        Value::Object(_) => Ok(payload),
        Value::String(raw) => serde_json::from_str::<Value>(raw.as_str())
            .map_err(|err| format!("invalid stored action_request payload: {}", err)),
        _ => Err("invalid action_request payload".to_string()),
    }
}

async fn load_ui_prompt_payload(prompt_id: &str) -> Result<UiPromptPayload, String> {
    if let Some(payload) = get_ui_prompt_payload(prompt_id).await {
        return Ok(payload);
    }

    let record = get_ui_prompt_record_by_id(prompt_id)
        .await?
        .ok_or_else(|| UI_PROMPT_NOT_FOUND_ERR.to_string())?;
    let record_prompt = record.prompt.clone();
    let mut payload = serde_json::from_value::<UiPromptPayload>(record_prompt.clone()).unwrap_or_else(|_| UiPromptPayload {
        prompt_id: record.id.clone(),
        session_id: record.session_id.clone(),
        conversation_turn_id: record.conversation_turn_id.clone(),
        tool_call_id: record.tool_call_id.clone(),
        kind: record.kind.clone(),
        title: record_prompt
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        message: record_prompt
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        allow_cancel: record_prompt
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: record_prompt
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(UI_PROMPT_TIMEOUT_MS_DEFAULT),
        payload: record_prompt
            .get("payload")
            .cloned()
            .or_else(|| if record_prompt.is_object() { Some(record_prompt.clone()) } else { None })
            .unwrap_or_else(|| json!({})),
    });
    if payload.prompt_id.trim().is_empty() {
        payload.prompt_id = record.id;
    }
    if payload.session_id.trim().is_empty() {
        payload.session_id = record.session_id;
    }
    if payload.conversation_turn_id.trim().is_empty() {
        payload.conversation_turn_id = record.conversation_turn_id;
    }
    if payload.kind.trim().is_empty() {
        payload.kind = record.kind;
    }
    Ok(payload)
}

use mongodb::bson::doc;

use crate::db::Db;
use crate::models::{
    AckPauseTaskRequest, AckStopTaskRequest, ContactTask, PauseTaskRequest, ResumeTaskRequest,
    RetryTaskRequest, StopTaskRequest,
};

use super::support::{
    apply_status_and_blocked_reason, build_handoff_payload, empty_task_update, normalize_optional_text,
    resolve_queue_position, runtimes, status_and_blocked_reason_for_dependencies, tasks,
};
use super::{get_task, update_task, upsert_scope_runtime};

async fn request_runtime_control(
    db: &Db,
    task: &ContactTask,
    control_request: &str,
    control_reason: Option<String>,
) -> Result<(), String> {
    upsert_scope_runtime(
        db,
        task.scope_key.as_str(),
        task.user_id.as_str(),
        task.contact_agent_id.as_str(),
        task.project_id.as_str(),
        Some(task.id.as_str()),
        Some(control_request.to_string()),
        Some(chrono::Utc::now().to_rfc3339()),
        control_reason,
        None,
        None,
    )
    .await
}

async fn clear_scope_runtime(db: &Db, task: &ContactTask) -> Result<(), String> {
    upsert_scope_runtime(
        db,
        task.scope_key.as_str(),
        task.user_id.as_str(),
        task.contact_agent_id.as_str(),
        task.project_id.as_str(),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
}

pub(super) async fn confirm_task(
    db: &Db,
    task_id: &str,
    note: Option<String>,
) -> Result<Option<ContactTask>, String> {
    let Some(task) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    if task
        .model_config_id
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        return Err("当前联系人未配置执行模型，无法进入待执行状态".to_string());
    }
    let queue_position = resolve_queue_position(db, &task).await?;
    let (next_status, blocked_reason) =
        status_and_blocked_reason_for_dependencies(task.depends_on_task_ids.as_slice());
    let mut update = empty_task_update();
    apply_status_and_blocked_reason(&mut update, next_status, blocked_reason);
    update.confirm_note = note;
    update.queue_position = Some(queue_position);
    update.last_checkpoint_summary = Some(None);
    update.last_checkpoint_message_id = Some(None);
    update.resume_note = Some(None);
    update_task(db, task_id, update).await
}

pub(super) async fn request_pause_task(
    db: &Db,
    task_id: &str,
    req: PauseTaskRequest,
) -> Result<Option<ContactTask>, String> {
    let Some(task) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    if task.status != "running" {
        return Err("only running tasks can accept a pause request".to_string());
    }
    let reason = normalize_optional_text(req.reason);
    request_runtime_control(db, &task, "pause", reason).await?;
    Ok(Some(task))
}

pub(super) async fn request_stop_task(
    db: &Db,
    task_id: &str,
    req: StopTaskRequest,
) -> Result<Option<ContactTask>, String> {
    let Some(task) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    if task.status != "running" {
        return Err("only running tasks can accept a stop request".to_string());
    }
    let reason = normalize_optional_text(req.reason);
    request_runtime_control(db, &task, "stop", reason).await?;
    Ok(Some(task))
}

pub(super) async fn ack_pause_task(
    db: &Db,
    task_id: &str,
    req: AckPauseTaskRequest,
) -> Result<Option<ContactTask>, String> {
    let Some(task) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    if task.status != "running" {
        return Err("only running tasks can be paused".to_string());
    }
    let checkpoint_summary = normalize_optional_text(req.checkpoint_summary);
    let checkpoint_message_id = normalize_optional_text(req.checkpoint_message_id);
    let runtime = runtimes(db)
        .find_one(doc! {"scope_key": task.scope_key.as_str()})
        .await
        .map_err(|e| e.to_string())?;
    let pause_reason = runtime.and_then(|item| item.control_reason);
    let mut update = empty_task_update();
    apply_status_and_blocked_reason(&mut update, "paused", None);
    update.handoff_payload = Some(build_handoff_payload(
        &task,
        "checkpoint",
        checkpoint_summary.as_deref(),
        checkpoint_summary.as_deref(),
        None,
        checkpoint_message_id.as_deref(),
        pause_reason.as_deref(),
    ));
    update.queue_position = Some(task.queue_position);
    update.pause_reason = Some(pause_reason);
    update.last_checkpoint_summary = Some(checkpoint_summary);
    update.last_checkpoint_message_id = Some(checkpoint_message_id);
    update.resume_note = Some(None);
    let updated = update_task(db, task_id, update).await?;
    clear_scope_runtime(db, &task).await?;
    Ok(updated)
}

pub(super) async fn ack_stop_task(
    db: &Db,
    task_id: &str,
    req: AckStopTaskRequest,
) -> Result<Option<ContactTask>, String> {
    let Some(task) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    if task.status != "running" {
        return Err("only running tasks can be stopped".to_string());
    }
    let result_summary = normalize_optional_text(req.result_summary);
    let result_message_id = normalize_optional_text(req.result_message_id);
    let last_error = normalize_optional_text(req.last_error);
    let mut update = empty_task_update();
    apply_status_and_blocked_reason(&mut update, "cancelled", None);
    update.handoff_payload = Some(build_handoff_payload(
        &task,
        "cancelled",
        result_summary.as_deref().or(last_error.as_deref()),
        result_summary.as_deref(),
        result_message_id.as_deref(),
        None,
        last_error.as_deref(),
    ));
    update.queue_position = Some(task.queue_position);
    update.pause_reason = Some(None);
    update.last_checkpoint_summary = Some(None);
    update.last_checkpoint_message_id = Some(None);
    update.resume_note = Some(None);
    update.result_summary = Some(result_summary);
    update.result_message_id = Some(result_message_id);
    update.last_error = Some(last_error);
    let updated = update_task(db, task_id, update).await?;
    clear_scope_runtime(db, &task).await?;
    Ok(updated)
}

pub(super) async fn resume_task(
    db: &Db,
    task_id: &str,
    req: ResumeTaskRequest,
) -> Result<Option<ContactTask>, String> {
    let Some(task) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    if task.status != "paused" {
        return Err("only paused tasks can be resumed".to_string());
    }
    let resume_note = normalize_optional_text(req.note);
    let queue_position = resolve_queue_position(db, &task).await?;
    let mut update = empty_task_update();
    apply_status_and_blocked_reason(&mut update, "pending_execute", None);
    update.queue_position = Some(queue_position);
    update.pause_reason = Some(None);
    update.resume_note = Some(resume_note);
    let updated = update_task(db, task_id, update).await?;
    upsert_scope_runtime(
        db,
        task.scope_key.as_str(),
        task.user_id.as_str(),
        task.contact_agent_id.as_str(),
        task.project_id.as_str(),
        None,
        None,
        None,
        None,
        Some(task.id.clone()),
        None,
    )
    .await?;
    Ok(updated)
}

pub(super) async fn retry_task(
    db: &Db,
    task_id: &str,
    req: RetryTaskRequest,
) -> Result<Option<ContactTask>, String> {
    let Some(task) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    if !matches!(task.status.as_str(), "failed" | "cancelled" | "skipped") {
        return Err("only failed, cancelled, or skipped tasks can be retried".to_string());
    }
    if task
        .model_config_id
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        return Err("当前联系人未配置执行模型，无法重新进入待执行状态".to_string());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let queue_position = resolve_queue_position(db, &task).await?;
    let (next_status, next_blocked_reason) =
        status_and_blocked_reason_for_dependencies(task.depends_on_task_ids.as_slice());

    let mut updated = task.clone();
    updated.blocked_reason = next_blocked_reason;
    updated.queue_position = queue_position;
    updated.status = next_status;
    updated.handoff_payload = None;
    updated.updated_at = now.clone();
    updated.confirmed_at = updated.confirmed_at.or_else(|| Some(now.clone()));
    updated.started_at = None;
    updated.paused_at = None;
    updated.pause_reason = None;
    updated.last_checkpoint_summary = None;
    updated.last_checkpoint_message_id = None;
    updated.resume_note = normalize_optional_text(req.note);
    updated.finished_at = None;
    updated.last_error = None;
    updated.result_summary = None;
    updated.result_message_id = None;

    tasks(db)
        .replace_one(doc! {"id": task_id}, updated.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(updated))
}

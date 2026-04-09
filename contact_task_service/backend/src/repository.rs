use std::collections::BTreeMap;

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    scope_key, AckPauseTaskRequest, AckStopTaskRequest, ContactTask, CreateTaskRequest,
    PauseTaskRequest, ResumeTaskRequest, RetryTaskRequest, SchedulerDecision, StopTaskRequest,
    TaskExecutionResultContract, TaskExecutionScopeView, TaskPlanView,
    UpdateTaskPlanRequest, UpdateTaskPlanResponse, UpdateTaskRequest,
};

mod support;
mod lifecycle;
mod scheduler;
mod plan_ops;
use self::support::{
    apply_status_and_blocked_reason, build_task_plan_view, build_tasks_filter, empty_task_update,
    list_plan_tasks, list_scope_tasks, list_tasks_by_ids, next_queue_position,
    normalize_builtin_mcp_ids, normalize_optional_text, normalize_string_list, runtimes, tasks,
};

async fn refresh_blocked_scope_tasks(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
) -> Result<(), String> {
    let blocked_tasks = list_scope_tasks(db, user_id, contact_agent_id, project_id, &["blocked"]).await?;
    for task in blocked_tasks {
        let dependency_tasks = list_tasks_by_ids(db, task.depends_on_task_ids.as_slice()).await?;
        let next_blocked_reason = if task.depends_on_task_ids.is_empty() {
            None
        } else if dependency_tasks.len() != task.depends_on_task_ids.len() {
            Some("dependency_missing".to_string())
        } else if dependency_tasks.iter().all(|item| item.status == "completed") {
            None
        } else if dependency_tasks
            .iter()
            .any(|item| item.status == "failed" || item.status == "cancelled" || item.status == "skipped")
        {
            Some("upstream_terminal_failure".to_string())
        } else {
            Some("waiting_for_dependencies".to_string())
        };
        let next_status = if next_blocked_reason.is_none() {
            "pending_execute"
        } else {
            "blocked"
        };
        let mut update = empty_task_update();
        apply_status_and_blocked_reason(&mut update, next_status, next_blocked_reason);
        update_task(db, task.id.as_str(), update).await?;
    }

    Ok(())
}

pub async fn create_task(
    db: &Db,
    scope_user_id: &str,
    auth_user_id: &str,
    req: CreateTaskRequest,
) -> Result<ContactTask, String> {
    let planned_builtin_mcp_ids = normalize_builtin_mcp_ids(req.planned_builtin_mcp_ids.as_slice());
    if planned_builtin_mcp_ids.is_empty() {
        return Err("planned_builtin_mcp_ids is required and cannot be empty".to_string());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let (priority, priority_rank) = crate::models::normalize_priority(req.priority.as_deref());
    let queue_position = next_queue_position(
        db,
        scope_user_id,
        req.contact_agent_id.as_str(),
        req.project_id.as_str(),
    )
    .await?;
    let item = ContactTask {
        id: Uuid::new_v4().to_string(),
        user_id: scope_user_id.to_string(),
        contact_agent_id: req.contact_agent_id.trim().to_string(),
        project_id: req.project_id.trim().to_string(),
        scope_key: scope_key(
            scope_user_id,
            req.contact_agent_id.as_str(),
            req.project_id.as_str(),
        ),
        task_plan_id: normalize_optional_text(req.task_plan_id),
        task_ref: normalize_optional_text(req.task_ref),
        task_kind: normalize_optional_text(req.task_kind),
        depends_on_task_ids: normalize_string_list(req.depends_on_task_ids),
        verification_of_task_ids: normalize_string_list(req.verification_of_task_ids),
        acceptance_criteria: normalize_string_list(req.acceptance_criteria),
        blocked_reason: None,
        project_root: normalize_optional_text(req.project_root),
        remote_connection_id: normalize_optional_text(req.remote_connection_id),
        session_id: req.session_id,
        conversation_turn_id: req.conversation_turn_id,
        source_message_id: req.source_message_id,
        model_config_id: req.model_config_id,
        title: req.title.trim().to_string(),
        content: req.content.trim().to_string(),
        priority,
        priority_rank,
        queue_position,
        status: "pending_confirm".to_string(),
        confirm_note: req.confirm_note,
        execution_note: req.execution_note,
        planned_builtin_mcp_ids,
        planned_context_assets: req.planned_context_assets,
        execution_result_contract: Some(req.execution_result_contract.unwrap_or(
            TaskExecutionResultContract {
                result_required: true,
                preferred_format: None,
            },
        )),
        planning_snapshot: req.planning_snapshot,
        handoff_payload: req.handoff_payload,
        created_by: Some(auth_user_id.to_string()),
        created_at: now.clone(),
        updated_at: now,
        confirmed_at: None,
        started_at: None,
        paused_at: None,
        pause_reason: None,
        last_checkpoint_summary: None,
        last_checkpoint_message_id: None,
        resume_note: None,
        finished_at: None,
        last_error: None,
        result_summary: None,
        result_message_id: None,
    };
    tasks(db)
        .insert_one(item.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(item)
}

pub async fn list_tasks(
    db: &Db,
    user_ids: &[String],
    contact_agent_id: Option<&str>,
    project_id: Option<&str>,
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ContactTask>, String> {
    let filter = build_tasks_filter(
        user_ids,
        contact_agent_id,
        project_id,
        session_id,
        conversation_turn_id,
        status,
    );

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();
    let cursor = tasks(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_task_plans(
    db: &Db,
    user_ids: &[String],
    contact_agent_id: Option<&str>,
    project_id: Option<&str>,
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<TaskPlanView>, String> {
    let filter = build_tasks_filter(
        user_ids,
        contact_agent_id,
        project_id,
        session_id,
        conversation_turn_id,
        status,
    );
    let cursor = tasks(db)
        .find(filter)
        .with_options(
            FindOptions::builder()
                .sort(doc! {"updated_at": -1, "created_at": -1, "id": -1})
                .limit(Some(5000))
                .build(),
        )
        .await
        .map_err(|e| e.to_string())?;
    let items: Vec<ContactTask> = cursor.try_collect().await.map_err(|e| e.to_string())?;

    let mut grouped = BTreeMap::<String, Vec<ContactTask>>::new();
    for task in items {
        let plan_id = task
            .task_plan_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| task.id.clone());
        grouped.entry(plan_id).or_default().push(task);
    }

    let mut plans = grouped
        .into_iter()
        .filter_map(|(_, items)| build_task_plan_view(items))
        .collect::<Vec<_>>();
    plans.sort_by(|left, right| right.latest_updated_at.cmp(&left.latest_updated_at));

    let safe_offset = offset.max(0) as usize;
    let safe_limit = limit.max(1).min(500) as usize;
    Ok(plans.into_iter().skip(safe_offset).take(safe_limit).collect())
}

pub async fn get_task_plan(
    db: &Db,
    plan_id: &str,
    user_ids: &[String],
) -> Result<Option<TaskPlanView>, String> {
    let items = list_plan_tasks(db, plan_id, user_ids).await?;
    Ok(build_task_plan_view(items))
}

pub async fn update_task_plan(
    db: &Db,
    plan_id: &str,
    req: UpdateTaskPlanRequest,
) -> Result<Option<UpdateTaskPlanResponse>, String> {
    plan_ops::update_task_plan(db, plan_id, req).await
}

pub async fn list_scheduler_scopes(
    db: &Db,
    user_ids: &[String],
    limit: i64,
) -> Result<Vec<TaskExecutionScopeView>, String> {
    let filter = if user_ids.is_empty() {
        doc! {
            "status": { "$in": ["pending_execute", "blocked", "running", "completed", "failed", "cancelled", "skipped"] }
        }
    } else if user_ids.len() == 1 {
        doc! {
            "user_id": user_ids[0].clone(),
            "status": { "$in": ["pending_execute", "blocked", "running", "completed", "failed", "cancelled", "skipped"] }
        }
    } else {
        doc! {
            "user_id": { "$in": user_ids },
            "status": { "$in": ["pending_execute", "blocked", "running", "completed", "failed", "cancelled", "skipped"] }
        }
    };

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1, "created_at": -1, "id": -1})
        .limit(Some(limit.max(1).min(2000)))
        .build();
    let cursor = tasks(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    let items: Vec<ContactTask> = cursor.try_collect().await.map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for task in items {
        if !seen.insert(task.scope_key.clone()) {
            continue;
        }
        out.push(TaskExecutionScopeView {
            scope_key: task.scope_key,
            user_id: task.user_id,
            contact_agent_id: task.contact_agent_id,
            project_id: task.project_id,
            latest_session_id: task.session_id,
            latest_task_id: Some(task.id),
            latest_task_updated_at: Some(task.updated_at),
        });
    }
    Ok(out)
}

pub async fn get_task(db: &Db, task_id: &str) -> Result<Option<ContactTask>, String> {
    tasks(db)
        .find_one(doc! {"id": task_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_task(
    db: &Db,
    task_id: &str,
    req: UpdateTaskRequest,
) -> Result<Option<ContactTask>, String> {
    let Some(existing) = get_task(db, task_id).await? else {
        return Ok(None);
    };
    let now = chrono::Utc::now().to_rfc3339();
    let (priority, priority_rank) = if let Some(priority) = req.priority.as_deref() {
        let normalized = crate::models::normalize_priority(Some(priority));
        (Some(normalized.0), Some(normalized.1))
    } else {
        (None, None)
    };

    let status = req.status.clone().unwrap_or(existing.status.clone());
    let finished_at = match status.as_str() {
        "completed" | "failed" | "cancelled" | "skipped" => Some(now.clone()),
        "pending_confirm" | "pending_execute" | "running" | "paused" | "blocked" => None,
        _ => existing.finished_at.clone(),
    };
    let confirmed_at = if status == "pending_execute" && existing.confirmed_at.is_none() {
        Some(now.clone())
    } else {
        existing.confirmed_at.clone()
    };
    let started_at = if status == "running" && existing.started_at.is_none() {
        Some(now.clone())
    } else if status != "running" && status != "paused" {
        existing.started_at.clone()
    } else {
        existing.started_at.clone()
    };
    let paused_at = if status == "paused" {
        Some(
            existing
                .paused_at
                .clone()
                .unwrap_or_else(|| now.clone()),
        )
    } else {
        None
    };
    let pause_reason = match req.pause_reason {
        Some(value) => value.and_then(|item| {
            let trimmed = item.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }),
        None => {
            if status == "paused" {
                existing.pause_reason.clone()
            } else {
                None
            }
        }
    };
    let last_checkpoint_summary = req
        .last_checkpoint_summary
        .unwrap_or(existing.last_checkpoint_summary.clone());
    let last_checkpoint_message_id = req
        .last_checkpoint_message_id
        .unwrap_or(existing.last_checkpoint_message_id.clone());
    let resume_note = req.resume_note.unwrap_or(existing.resume_note.clone());
    let queue_position = req.queue_position.unwrap_or(existing.queue_position);
    let task_ref = req.task_ref.unwrap_or(existing.task_ref.clone());
    let task_kind = req.task_kind.unwrap_or(existing.task_kind.clone());
    let depends_on_task_ids = req
        .depends_on_task_ids
        .map(normalize_string_list)
        .unwrap_or(existing.depends_on_task_ids.clone());
    let verification_of_task_ids = req
        .verification_of_task_ids
        .map(normalize_string_list)
        .unwrap_or(existing.verification_of_task_ids.clone());
    let acceptance_criteria = req
        .acceptance_criteria
        .map(normalize_string_list)
        .unwrap_or(existing.acceptance_criteria.clone());
    let blocked_reason = req.blocked_reason.unwrap_or(existing.blocked_reason.clone());

    let updated = ContactTask {
        id: existing.id.clone(),
        user_id: existing.user_id.clone(),
        contact_agent_id: existing.contact_agent_id.clone(),
        project_id: existing.project_id.clone(),
        scope_key: existing.scope_key.clone(),
        task_plan_id: existing.task_plan_id.clone(),
        task_ref,
        task_kind,
        depends_on_task_ids,
        verification_of_task_ids,
        acceptance_criteria,
        blocked_reason,
        project_root: req
            .project_root
            .map(normalize_optional_text)
            .unwrap_or(existing.project_root.clone()),
        remote_connection_id: req
            .remote_connection_id
            .map(normalize_optional_text)
            .unwrap_or(existing.remote_connection_id.clone()),
        session_id: existing.session_id.clone(),
        conversation_turn_id: existing.conversation_turn_id.clone(),
        source_message_id: existing.source_message_id.clone(),
        model_config_id: req
            .model_config_id
            .unwrap_or(existing.model_config_id.clone()),
        title: req.title.unwrap_or(existing.title.clone()),
        content: req.content.unwrap_or(existing.content.clone()),
        priority: priority.unwrap_or(existing.priority.clone()),
        priority_rank: priority_rank.unwrap_or(existing.priority_rank),
        queue_position,
        status,
        confirm_note: req.confirm_note.or(existing.confirm_note.clone()),
        execution_note: req.execution_note.or(existing.execution_note.clone()),
        planned_builtin_mcp_ids: req
            .planned_builtin_mcp_ids
            .map(|ids| normalize_builtin_mcp_ids(ids.as_slice()))
            .unwrap_or(existing.planned_builtin_mcp_ids.clone()),
        planned_context_assets: req
            .planned_context_assets
            .unwrap_or(existing.planned_context_assets.clone()),
        execution_result_contract: req
            .execution_result_contract
            .or(existing.execution_result_contract.clone()),
        planning_snapshot: req.planning_snapshot.or(existing.planning_snapshot.clone()),
        handoff_payload: req.handoff_payload.unwrap_or(existing.handoff_payload.clone()),
        created_by: existing.created_by.clone(),
        created_at: existing.created_at.clone(),
        updated_at: now.clone(),
        confirmed_at,
        started_at,
        paused_at,
        pause_reason,
        last_checkpoint_summary,
        last_checkpoint_message_id,
        resume_note,
        finished_at,
        last_error: req.last_error.unwrap_or(existing.last_error.clone()),
        result_summary: req
            .result_summary
            .unwrap_or(existing.result_summary.clone()),
        result_message_id: req
            .result_message_id
            .unwrap_or(existing.result_message_id.clone()),
    };

    tasks(db)
        .replace_one(doc! {"id": task_id}, updated.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(updated))
}

pub async fn delete_task(db: &Db, task_id: &str) -> Result<bool, String> {
    let result = tasks(db)
        .delete_one(doc! {"id": task_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn confirm_task(
    db: &Db,
    task_id: &str,
    note: Option<String>,
) -> Result<Option<ContactTask>, String> {
    lifecycle::confirm_task(db, task_id, note).await
}

pub async fn scheduler_next(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
) -> Result<SchedulerDecision, String> {
    scheduler::scheduler_next(db, user_id, contact_agent_id, project_id).await
}

pub async fn ack_all_done(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    ack_at: &str,
) -> Result<(), String> {
    scheduler::ack_all_done(db, user_id, contact_agent_id, project_id, ack_at).await
}

pub async fn request_pause_task(
    db: &Db,
    task_id: &str,
    req: PauseTaskRequest,
) -> Result<Option<ContactTask>, String> {
    lifecycle::request_pause_task(db, task_id, req).await
}

pub async fn request_stop_task(
    db: &Db,
    task_id: &str,
    req: StopTaskRequest,
) -> Result<Option<ContactTask>, String> {
    lifecycle::request_stop_task(db, task_id, req).await
}

pub async fn ack_pause_task(
    db: &Db,
    task_id: &str,
    req: AckPauseTaskRequest,
) -> Result<Option<ContactTask>, String> {
    lifecycle::ack_pause_task(db, task_id, req).await
}

pub async fn ack_stop_task(
    db: &Db,
    task_id: &str,
    req: AckStopTaskRequest,
) -> Result<Option<ContactTask>, String> {
    lifecycle::ack_stop_task(db, task_id, req).await
}

pub async fn resume_task(
    db: &Db,
    task_id: &str,
    req: ResumeTaskRequest,
) -> Result<Option<ContactTask>, String> {
    lifecycle::resume_task(db, task_id, req).await
}

pub async fn retry_task(
    db: &Db,
    task_id: &str,
    req: RetryTaskRequest,
) -> Result<Option<ContactTask>, String> {
    lifecycle::retry_task(db, task_id, req).await
}

async fn upsert_scope_runtime(
    db: &Db,
    key: &str,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    running_task_id: Option<&str>,
    control_request: Option<String>,
    control_requested_at: Option<String>,
    control_reason: Option<String>,
    resume_target_task_id: Option<String>,
    last_all_done_ack_at: Option<String>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    runtimes(db)
        .update_one(
            doc! {"scope_key": key},
            doc! {"$set": {
                "scope_key": key,
                "user_id": user_id,
                "contact_agent_id": contact_agent_id,
                "project_id": project_id,
                "running_task_id": running_task_id,
                "control_request": control_request,
                "control_requested_at": control_requested_at,
                "control_reason": control_reason,
                "resume_target_task_id": resume_target_task_id,
                "last_all_done_ack_at": last_all_done_ack_at,
                "updated_at": now,
            }},
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

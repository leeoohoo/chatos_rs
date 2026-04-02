use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    scope_key, ContactTask, ContactTaskScopeRuntime, CreateTaskRequest, SchedulerDecision,
    TaskExecutionScopeView, UpdateTaskRequest,
};

fn tasks(db: &Db) -> mongodb::Collection<ContactTask> {
    db.collection::<ContactTask>("contact_tasks")
}

fn runtimes(db: &Db) -> mongodb::Collection<ContactTaskScopeRuntime> {
    db.collection::<ContactTaskScopeRuntime>("contact_task_scope_runtimes")
}

pub async fn create_task(
    db: &Db,
    scope_user_id: &str,
    auth_user_id: &str,
    req: CreateTaskRequest,
) -> Result<ContactTask, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let (priority, priority_rank) = crate::models::normalize_priority(req.priority.as_deref());
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
        session_id: req.session_id,
        conversation_turn_id: req.conversation_turn_id,
        source_message_id: req.source_message_id,
        model_config_id: req.model_config_id,
        title: req.title.trim().to_string(),
        content: req.content.trim().to_string(),
        priority,
        priority_rank,
        status: "pending_confirm".to_string(),
        confirm_note: req.confirm_note,
        execution_note: req.execution_note,
        created_by: Some(auth_user_id.to_string()),
        created_at: now.clone(),
        updated_at: now,
        confirmed_at: None,
        started_at: None,
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
    let mut filter = if user_ids.is_empty() {
        doc! {}
    } else if user_ids.len() == 1 {
        doc! { "user_id": user_ids[0].clone() }
    } else {
        doc! { "user_id": { "$in": user_ids } }
    };
    if let Some(value) = contact_agent_id {
        filter.insert("contact_agent_id", value.trim());
    }
    if let Some(value) = project_id {
        filter.insert("project_id", value.trim());
    }
    if let Some(value) = session_id {
        filter.insert("session_id", value.trim());
    }
    if let Some(value) = conversation_turn_id {
        filter.insert("conversation_turn_id", value.trim());
    }
    if let Some(value) = status {
        filter.insert("status", value.trim());
    }

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

pub async fn list_scheduler_scopes(
    db: &Db,
    user_ids: &[String],
    limit: i64,
) -> Result<Vec<TaskExecutionScopeView>, String> {
    let filter = if user_ids.is_empty() {
        doc! {
            "status": { "$in": ["pending_execute", "running", "completed", "failed", "cancelled"] }
        }
    } else if user_ids.len() == 1 {
        doc! {
            "user_id": user_ids[0].clone(),
            "status": { "$in": ["pending_execute", "running", "completed", "failed", "cancelled"] }
        }
    } else {
        doc! {
            "user_id": { "$in": user_ids },
            "status": { "$in": ["pending_execute", "running", "completed", "failed", "cancelled"] }
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
        "completed" | "failed" | "cancelled" => Some(now.clone()),
        _ => existing.finished_at.clone(),
    };
    let confirmed_at = if status == "pending_execute" && existing.confirmed_at.is_none() {
        Some(now.clone())
    } else {
        existing.confirmed_at.clone()
    };

    let updated = ContactTask {
        id: existing.id.clone(),
        user_id: existing.user_id.clone(),
        contact_agent_id: existing.contact_agent_id.clone(),
        project_id: existing.project_id.clone(),
        scope_key: existing.scope_key.clone(),
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
        status,
        confirm_note: req.confirm_note.or(existing.confirm_note.clone()),
        execution_note: req.execution_note.or(existing.execution_note.clone()),
        created_by: existing.created_by.clone(),
        created_at: existing.created_at.clone(),
        updated_at: now.clone(),
        confirmed_at,
        started_at: existing.started_at.clone(),
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
    update_task(
        db,
        task_id,
        UpdateTaskRequest {
            title: None,
            content: None,
            priority: None,
            status: Some("pending_execute".to_string()),
            confirm_note: note,
            execution_note: None,
            model_config_id: None,
            result_summary: None,
            result_message_id: None,
            last_error: None,
        },
    )
    .await
}

pub async fn scheduler_next(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
) -> Result<SchedulerDecision, String> {
    let key = scope_key(user_id, contact_agent_id, project_id);
    let runtime = runtimes(db)
        .find_one(doc! {"scope_key": &key})
        .await
        .map_err(|e| e.to_string())?;

    if let Some(runtime) = runtime.as_ref() {
        if let Some(running_task_id) = runtime.running_task_id.as_deref() {
            if let Some(task) = get_task(db, running_task_id).await? {
                if task.status == "running" {
                    return Ok(SchedulerDecision {
                        decision: "pass".to_string(),
                        task: None,
                        scope_key: key,
                    });
                }
            }
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let options = FindOneAndUpdateOptions::builder()
        .sort(doc! {"priority_rank": 1, "created_at": 1, "id": 1})
        .return_document(Some(ReturnDocument::After))
        .build();
    let next_task = tasks(db)
        .find_one_and_update(
            doc! {
                "user_id": user_id,
                "contact_agent_id": contact_agent_id,
                "project_id": project_id,
                "status": "pending_execute",
            },
            doc! {
                "$set": {
                    "status": "running",
                    "started_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(task) = next_task {
        upsert_scope_runtime(
            db,
            &key,
            user_id,
            contact_agent_id,
            project_id,
            Some(task.id.as_str()),
            runtime.and_then(|item| item.last_all_done_ack_at),
        )
        .await?;
        return Ok(SchedulerDecision {
            decision: "task".to_string(),
            task: Some(task),
            scope_key: key,
        });
    }

    let last_terminal = tasks(db)
        .find_one(doc! {
            "user_id": user_id,
            "contact_agent_id": contact_agent_id,
            "project_id": project_id,
            "status": {"$in": ["completed", "failed", "cancelled"]},
        })
        .sort(doc! {"updated_at": -1})
        .await
        .map_err(|e| e.to_string())?;

    let ack_at = runtime.and_then(|item| item.last_all_done_ack_at);
    if let Some(task) = last_terminal {
        let should_send_all_done = ack_at
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|ack| task.updated_at.as_str() > ack)
            .unwrap_or(true);
        if should_send_all_done {
            return Ok(SchedulerDecision {
                decision: "all_done".to_string(),
                task: None,
                scope_key: key,
            });
        }
    }

    Ok(SchedulerDecision {
        decision: "pass".to_string(),
        task: None,
        scope_key: key,
    })
}

pub async fn ack_all_done(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    ack_at: &str,
) -> Result<(), String> {
    let key = scope_key(user_id, contact_agent_id, project_id);
    let existing = runtimes(db)
        .find_one(doc! {"scope_key": &key})
        .await
        .map_err(|e| e.to_string())?;
    upsert_scope_runtime(
        db,
        key.as_str(),
        user_id,
        contact_agent_id,
        project_id,
        existing.and_then(|item| item.running_task_id).as_deref(),
        Some(ack_at.to_string()),
    )
    .await
}

async fn upsert_scope_runtime(
    db: &Db,
    key: &str,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    running_task_id: Option<&str>,
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
                "last_all_done_ack_at": last_all_done_ack_at,
                "updated_at": now,
            }},
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

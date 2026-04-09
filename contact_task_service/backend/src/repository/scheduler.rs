use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};

use crate::db::Db;
use crate::models::SchedulerDecision;

use super::support::{list_scope_tasks, runtimes, tasks};
use super::{refresh_blocked_scope_tasks, upsert_scope_runtime};

pub(super) async fn scheduler_next(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
) -> Result<SchedulerDecision, String> {
    let key = crate::models::scope_key(user_id, contact_agent_id, project_id);
    let runtime = runtimes(db)
        .find_one(doc! {"scope_key": &key})
        .await
        .map_err(|e| e.to_string())?;
    let ack_at = runtime
        .as_ref()
        .and_then(|item| item.last_all_done_ack_at.clone());

    if let Some(runtime) = runtime.as_ref() {
        let control_request_active = runtime
            .control_request
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some();
        let mut has_stale_running_task_marker = false;
        if let Some(running_task_id) = runtime
            .running_task_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Some(task) = super::get_task(db, running_task_id).await? {
                let is_same_scope = task.user_id == user_id
                    && task.contact_agent_id == contact_agent_id
                    && task.project_id == project_id;
                if task.status == "running" && is_same_scope {
                    return Ok(SchedulerDecision {
                        decision: "pass".to_string(),
                        task: None,
                        scope_key: key,
                    });
                }
                has_stale_running_task_marker = true;
            } else {
                has_stale_running_task_marker = true;
            }
        }

        let has_control_marker = control_request_active
            || runtime
                .control_requested_at
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
            || runtime
                .control_reason
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();
        let has_stale_runtime_marker = has_stale_running_task_marker || has_control_marker;

        if has_stale_runtime_marker {
            upsert_scope_runtime(
                db,
                &key,
                user_id,
                contact_agent_id,
                project_id,
                None,
                None,
                None,
                None,
                runtime.resume_target_task_id.clone(),
                runtime.last_all_done_ack_at.clone(),
            )
            .await?;
        }
    }

    refresh_blocked_scope_tasks(db, user_id, contact_agent_id, project_id).await?;

    let paused_tasks = list_scope_tasks(db, user_id, contact_agent_id, project_id, &["paused"]).await?;
    if !paused_tasks.is_empty() {
        return Ok(SchedulerDecision {
            decision: "await_resume".to_string(),
            task: None,
            scope_key: key,
        });
    }

    let now = chrono::Utc::now().to_rfc3339();
    let options = FindOneAndUpdateOptions::builder()
        .sort(doc! {"queue_position": 1, "priority_rank": 1, "created_at": 1, "id": 1})
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
                    "paused_at": mongodb::bson::Bson::Null,
                    "pause_reason": mongodb::bson::Bson::Null,
                    "resume_note": mongodb::bson::Bson::Null,
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
            None,
            None,
            None,
            None,
            ack_at.clone(),
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
            "status": {"$in": ["completed", "failed", "cancelled", "skipped"]},
        })
        .sort(doc! {"updated_at": -1})
        .await
        .map_err(|e| e.to_string())?;

    let unfinished_count = tasks(db)
        .count_documents(doc! {
            "user_id": user_id,
            "contact_agent_id": contact_agent_id,
            "project_id": project_id,
            "status": {"$in": ["pending_confirm", "pending_execute", "running", "paused", "blocked"]},
        })
        .await
        .map_err(|e| e.to_string())?;
    if unfinished_count > 0 {
        return Ok(SchedulerDecision {
            decision: "pass".to_string(),
            task: None,
            scope_key: key,
        });
    }

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

pub(super) async fn ack_all_done(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    ack_at: &str,
) -> Result<(), String> {
    let key = crate::models::scope_key(user_id, contact_agent_id, project_id);
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
        existing
            .as_ref()
            .and_then(|item| item.running_task_id.as_deref()),
        existing
            .as_ref()
            .and_then(|item| item.control_request.clone()),
        existing
            .as_ref()
            .and_then(|item| item.control_requested_at.clone()),
        existing.as_ref().and_then(|item| item.control_reason.clone()),
        existing
            .as_ref()
            .and_then(|item| item.resume_target_task_id.clone()),
        Some(ack_at.to_string()),
    )
    .await
}

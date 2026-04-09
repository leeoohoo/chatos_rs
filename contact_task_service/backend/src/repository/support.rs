use std::collections::{BTreeMap, HashSet};

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::{ContactTask, ContactTaskScopeRuntime, TaskHandoffPayload, TaskPlanView, UpdateTaskRequest};

pub(super) fn normalize_builtin_mcp_ids(ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for item in ids {
        let trimmed = item.trim();
        if trimmed.is_empty() || out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

pub(super) fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

pub(super) fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

pub(super) fn build_handoff_payload(
    task: &ContactTask,
    handoff_kind: &str,
    summary: Option<&str>,
    result_summary: Option<&str>,
    result_message_id: Option<&str>,
    checkpoint_message_id: Option<&str>,
    open_risk: Option<&str>,
) -> Option<TaskHandoffPayload> {
    let summary_text = summary
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| result_summary.map(str::trim).filter(|value| !value.is_empty()))?
        .to_string();

    let task_kind = task
        .task_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("task");
    let mut key_changes = Vec::new();
    match handoff_kind.trim() {
        "checkpoint" => key_changes.push(format!("{} 已暂停：{}", task_kind, task.title)),
        "cancelled" => key_changes.push(format!("{} 已停止：{}", task_kind, task.title)),
        _ => key_changes.push(format!("{} 状态更新：{}", task_kind, task.title)),
    }

    let verification_suggestions = if task.acceptance_criteria.is_empty() {
        Vec::new()
    } else {
        task.acceptance_criteria.clone()
    };

    let open_risks = open_risk
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();

    let mut artifact_refs = Vec::new();
    if let Some(session_id) = task
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        artifact_refs.push(format!("session:{}", session_id));
    }
    if let Some(turn_id) = task
        .conversation_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        artifact_refs.push(format!("turn:{}", turn_id));
    }
    if let Some(message_id) = result_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        artifact_refs.push(format!("result_message:{}", message_id));
    }

    let checkpoint_message_ids = checkpoint_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();

    Some(TaskHandoffPayload {
        task_id: task.id.clone(),
        task_plan_id: task.task_plan_id.clone(),
        handoff_kind: handoff_kind.trim().to_string(),
        summary: summary_text.clone(),
        result_summary: result_summary
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        key_changes,
        changed_files: Vec::new(),
        executed_commands: Vec::new(),
        verification_suggestions,
        open_risks,
        artifact_refs,
        checkpoint_message_ids,
        result_brief_id: None,
        generated_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub(super) fn tasks(db: &Db) -> mongodb::Collection<ContactTask> {
    db.collection::<ContactTask>("contact_tasks")
}

pub(super) fn runtimes(db: &Db) -> mongodb::Collection<ContactTaskScopeRuntime> {
    db.collection::<ContactTaskScopeRuntime>("contact_task_scope_runtimes")
}

pub(super) async fn next_queue_position(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
) -> Result<i64, String> {
    let task = tasks(db)
        .find_one(doc! {
            "user_id": user_id,
            "contact_agent_id": contact_agent_id,
            "project_id": project_id,
        })
        .sort(doc! {"queue_position": -1, "created_at": -1, "id": -1})
        .await
        .map_err(|e| e.to_string())?;
    Ok(task.map(|item| item.queue_position.max(0) + 1).unwrap_or(1))
}

pub(super) async fn list_scope_tasks(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    statuses: &[&str],
) -> Result<Vec<ContactTask>, String> {
    let filter = if statuses.is_empty() {
        doc! {
            "user_id": user_id,
            "contact_agent_id": contact_agent_id,
            "project_id": project_id,
        }
    } else {
        doc! {
            "user_id": user_id,
            "contact_agent_id": contact_agent_id,
            "project_id": project_id,
            "status": {"$in": statuses},
        }
    };
    let cursor = tasks(db)
        .find(filter)
        .with_options(
            FindOptions::builder()
                .sort(doc! {"queue_position": 1, "priority_rank": 1, "created_at": 1, "id": 1})
                .build(),
        )
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub(super) async fn list_tasks_by_ids(db: &Db, task_ids: &[String]) -> Result<Vec<ContactTask>, String> {
    let normalized_ids = normalize_string_list(task_ids.to_vec());
    if normalized_ids.is_empty() {
        return Ok(Vec::new());
    }
    let cursor = tasks(db)
        .find(doc! {
            "id": { "$in": normalized_ids },
        })
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub(super) fn sort_plan_tasks(items: &mut [ContactTask]) {
    items.sort_by(|left, right| {
        left.queue_position
            .cmp(&right.queue_position)
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn build_plan_filter(plan_id: &str) -> mongodb::bson::Document {
    doc! {
        "$or": [
            { "task_plan_id": plan_id },
            { "task_plan_id": null, "id": plan_id },
            { "task_plan_id": "", "id": plan_id },
        ]
    }
}

pub(super) async fn list_plan_tasks(
    db: &Db,
    plan_id: &str,
    user_ids: &[String],
) -> Result<Vec<ContactTask>, String> {
    let normalized_plan_id = plan_id.trim();
    if normalized_plan_id.is_empty() {
        return Ok(Vec::new());
    }
    let plan_filter = build_plan_filter(normalized_plan_id);
    let filter = if user_ids.is_empty() {
        plan_filter
    } else if user_ids.len() == 1 {
        doc! {
            "$and": [
                { "user_id": user_ids[0].clone() },
                plan_filter,
            ]
        }
    } else {
        doc! {
            "$and": [
                { "user_id": { "$in": user_ids } },
                plan_filter,
            ]
        }
    };

    let cursor = tasks(db)
        .find(filter)
        .with_options(
            FindOptions::builder()
                .sort(doc! {"queue_position": 1, "created_at": 1, "id": 1})
                .build(),
        )
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub(super) fn build_task_plan_view(mut items: Vec<ContactTask>) -> Option<TaskPlanView> {
    if items.is_empty() {
        return None;
    }
    sort_plan_tasks(&mut items);
    let first = items.first()?.clone();
    let plan_id = first
        .task_plan_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| first.id.clone());
    let latest_updated_at = items
        .iter()
        .map(|task| task.updated_at.clone())
        .max()
        .unwrap_or_else(|| first.updated_at.clone());
    let active_task_id = items
        .iter()
        .find(|task| task.status == "running")
        .or_else(|| items.iter().find(|task| task.status == "pending_execute"))
        .or_else(|| items.iter().find(|task| task.status == "blocked"))
        .or_else(|| items.iter().find(|task| task.status == "paused"))
        .map(|task| task.id.clone());
    let mut status_counts = BTreeMap::new();
    let mut blocked_task_count = 0_i64;
    for task in &items {
        *status_counts.entry(task.status.clone()).or_insert(0) += 1;
        if task.status == "blocked" || task.blocked_reason.is_some() {
            blocked_task_count += 1;
        }
    }

    Some(TaskPlanView {
        plan_id,
        user_id: first.user_id,
        contact_agent_id: first.contact_agent_id,
        project_id: first.project_id,
        title: first.title,
        task_count: items.len() as i64,
        blocked_task_count,
        latest_updated_at,
        active_task_id,
        status_counts,
        tasks: items,
    })
}

pub(super) fn build_direct_dependents_map(items: &[ContactTask]) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::<String, Vec<String>>::new();
    for task in items {
        for dependency_task_id in &task.depends_on_task_ids {
            let existing = out.entry(dependency_task_id.clone()).or_default();
            if !existing.contains(&task.id) {
                existing.push(task.id.clone());
            }
        }
    }
    out
}

pub(super) fn collect_descendant_ids(
    direct_dependents_by_task_id: &BTreeMap<String, Vec<String>>,
    task_id: &str,
) -> Vec<String> {
    fn visit(
        direct_dependents_by_task_id: &BTreeMap<String, Vec<String>>,
        task_id: &str,
        seen: &mut HashSet<String>,
    ) {
        for next_id in direct_dependents_by_task_id
            .get(task_id)
            .cloned()
            .unwrap_or_default()
        {
            if !seen.insert(next_id.clone()) {
                continue;
            }
            visit(direct_dependents_by_task_id, next_id.as_str(), seen);
        }
    }

    let mut seen = HashSet::new();
    visit(direct_dependents_by_task_id, task_id, &mut seen);
    seen.into_iter().collect()
}

pub(super) fn is_terminal_plan_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "failed" | "cancelled" | "skipped"
    )
}

pub(super) fn build_tasks_filter(
    user_ids: &[String],
    contact_agent_id: Option<&str>,
    project_id: Option<&str>,
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    status: Option<&str>,
) -> mongodb::bson::Document {
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
    filter
}

pub(super) fn empty_task_update() -> UpdateTaskRequest {
    UpdateTaskRequest {
        title: None,
        content: None,
        priority: None,
        status: None,
        task_ref: None,
        task_kind: None,
        depends_on_task_ids: None,
        verification_of_task_ids: None,
        acceptance_criteria: None,
        blocked_reason: None,
        confirm_note: None,
        execution_note: None,
        project_root: None,
        remote_connection_id: None,
        planned_builtin_mcp_ids: None,
        planned_context_assets: None,
        execution_result_contract: None,
        planning_snapshot: None,
        handoff_payload: None,
        model_config_id: None,
        queue_position: None,
        pause_reason: None,
        last_checkpoint_summary: None,
        last_checkpoint_message_id: None,
        resume_note: None,
        result_summary: None,
        result_message_id: None,
        last_error: None,
    }
}

pub(super) fn apply_status_and_blocked_reason(
    update: &mut UpdateTaskRequest,
    status: impl Into<String>,
    blocked_reason: Option<String>,
) {
    update.status = Some(status.into());
    update.blocked_reason = Some(blocked_reason);
}

pub(super) fn status_and_blocked_reason_for_dependencies(
    depends_on_task_ids: &[String],
) -> (String, Option<String>) {
    if depends_on_task_ids.is_empty() {
        ("pending_execute".to_string(), None)
    } else {
        ("blocked".to_string(), Some("waiting_for_dependencies".to_string()))
    }
}

pub(super) async fn resolve_queue_position(db: &Db, task: &ContactTask) -> Result<i64, String> {
    if task.queue_position > 0 {
        return Ok(task.queue_position);
    }
    next_queue_position(
        db,
        task.user_id.as_str(),
        task.contact_agent_id.as_str(),
        task.project_id.as_str(),
    )
    .await
}

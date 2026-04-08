use std::collections::{BTreeMap, HashSet};

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    scope_key, AckPauseTaskRequest, AckStopTaskRequest, ContactTask, ContactTaskScopeRuntime,
    CreateTaskRequest, PauseTaskRequest, ResumeTaskRequest, RetryTaskRequest,
    SchedulerDecision, StopTaskRequest, TaskExecutionResultContract, TaskExecutionScopeView, TaskHandoffPayload,
    TaskPlanOperationResult, TaskPlanView, UpdateTaskPlanRequest, UpdateTaskPlanResponse,
    UpdateTaskRequest,
};

fn normalize_builtin_mcp_ids(ids: &[String]) -> Vec<String> {
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

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
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

fn build_handoff_payload(
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

fn tasks(db: &Db) -> mongodb::Collection<ContactTask> {
    db.collection::<ContactTask>("contact_tasks")
}

fn runtimes(db: &Db) -> mongodb::Collection<ContactTaskScopeRuntime> {
    db.collection::<ContactTaskScopeRuntime>("contact_task_scope_runtimes")
}

async fn next_queue_position(
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

async fn list_scope_tasks(
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

async fn list_tasks_by_ids(db: &Db, task_ids: &[String]) -> Result<Vec<ContactTask>, String> {
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

fn sort_plan_tasks(items: &mut [ContactTask]) {
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

async fn list_plan_tasks(
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

fn build_task_plan_view(mut items: Vec<ContactTask>) -> Option<TaskPlanView> {
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

fn empty_task_update() -> UpdateTaskRequest {
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

fn build_direct_dependents_map(items: &[ContactTask]) -> BTreeMap<String, Vec<String>> {
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

fn collect_descendant_ids(
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

fn is_terminal_plan_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "failed" | "cancelled" | "skipped"
    )
}

fn build_tasks_filter(
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
            "pending_execute".to_string()
        } else {
            "blocked".to_string()
        };
        let blocked_reason_patch = Some(next_blocked_reason);
        update_task(
            db,
            task.id.as_str(),
            UpdateTaskRequest {
                title: None,
                content: None,
                priority: None,
                status: Some(next_status),
                task_ref: None,
                task_kind: None,
                depends_on_task_ids: None,
                verification_of_task_ids: None,
                acceptance_criteria: None,
                blocked_reason: blocked_reason_patch,
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
            },
        )
        .await?;
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
    let mut plan_tasks = list_plan_tasks(db, plan_id, &[]).await?;
    if plan_tasks.is_empty() {
        return Ok(None);
    }
    sort_plan_tasks(&mut plan_tasks);

    let task_ids: HashSet<String> = plan_tasks.iter().map(|task| task.id.clone()).collect();
    let scope_keys: HashSet<(String, String, String)> = plan_tasks
        .iter()
        .map(|task| {
            (
                task.user_id.clone(),
                task.contact_agent_id.clone(),
                task.project_id.clone(),
            )
        })
        .collect();
    let task_lookup = plan_tasks
        .iter()
        .map(|task| (task.id.clone(), task.clone()))
        .collect::<BTreeMap<_, _>>();
    let direct_dependents_by_task_id = build_direct_dependents_map(plan_tasks.as_slice());

    let ordered_task_ids = normalize_string_list(req.ordered_task_ids);
    if !ordered_task_ids.is_empty() {
        let ordered_set: HashSet<String> = ordered_task_ids.iter().cloned().collect();
        if ordered_task_ids.len() != plan_tasks.len() || ordered_set.len() != plan_tasks.len() {
            return Err("ordered_task_ids 必须完整覆盖该计划内的全部任务".to_string());
        }
        if ordered_set != task_ids {
            return Err("ordered_task_ids 中存在不属于当前计划的任务".to_string());
        }
        for (index, task_id) in ordered_task_ids.iter().enumerate() {
            let mut update = empty_task_update();
            update.queue_position = Some((index + 1) as i64);
            update_task(db, task_id.as_str(), update).await?;
        }
    }

    let mut seen_operation_targets = HashSet::new();
    let mut operation_results = Vec::new();
    for operation in req.operations {
        let kind = operation.kind.trim().to_string();
        let task_id = operation.task_id.trim().to_string();
        if task_id.is_empty() {
            return Err("operation.task_id 不能为空".to_string());
        }
        if !task_ids.contains(task_id.as_str()) {
            return Err(format!("任务 {} 不属于当前计划", task_id));
        }
        if !seen_operation_targets.insert(format!("{}:{}", kind, task_id)) {
            return Err(format!("任务 {} 的计划操作重复出现", task_id));
        }

        match kind.as_str() {
            "skip_with_descendants" => {
                let mut affected_ids = vec![task_id.clone()];
                affected_ids.extend(collect_descendant_ids(&direct_dependents_by_task_id, task_id.as_str()));
                let mut updated_task_ids = Vec::new();
                for affected_id in affected_ids {
                    let Some(task) = task_lookup.get(affected_id.as_str()) else {
                        continue;
                    };
                    if task.status == "running" || is_terminal_plan_status(task.status.as_str()) {
                        continue;
                    }
                    let mut update = empty_task_update();
                    update.status = Some("skipped".to_string());
                    update.blocked_reason = Some(None);
                    update_task(db, affected_id.as_str(), update).await?;
                    updated_task_ids.push(affected_id);
                }
                operation_results.push(TaskPlanOperationResult {
                    kind,
                    task_id,
                    affected_count: updated_task_ids.len() as i64,
                    affected_task_ids: updated_task_ids,
                    replacement_task_id: None,
                });
            }
            "rewire_direct_dependents" => {
                let replacement_task_id = operation
                    .replacement_task_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string());
                if let Some(replacement_task_id) = replacement_task_id.as_ref() {
                    if !task_ids.contains(replacement_task_id.as_str()) {
                        return Err(format!("replacement_task_id {} 不属于当前计划", replacement_task_id));
                    }
                    if replacement_task_id == &task_id {
                        return Err("replacement_task_id 不能等于源节点".to_string());
                    }
                }
                let mut updated_task_ids = Vec::new();
                for dependent_id in direct_dependents_by_task_id
                    .get(task_id.as_str())
                    .cloned()
                    .unwrap_or_default()
                {
                    let Some(task) = task_lookup.get(dependent_id.as_str()) else {
                        continue;
                    };
                    if task.status == "running" || is_terminal_plan_status(task.status.as_str()) {
                        continue;
                    }
                    if let Some(replacement_task_id) = replacement_task_id.as_ref() {
                        if replacement_task_id == &dependent_id {
                            return Err(format!("任务 {} 不能把自己作为新的前置依赖", dependent_id));
                        }
                        let descendant_ids =
                            collect_descendant_ids(&direct_dependents_by_task_id, dependent_id.as_str());
                        if descendant_ids.iter().any(|item| item == replacement_task_id) {
                            return Err(format!(
                                "任务 {} 不能重挂到自己的后继节点 {}",
                                dependent_id, replacement_task_id
                            ));
                        }
                    }
                    let mut next_depends = task
                        .depends_on_task_ids
                        .iter()
                        .filter(|item| item.as_str() != task_id.as_str())
                        .cloned()
                        .collect::<Vec<_>>();
                    if let Some(replacement_task_id) = replacement_task_id.as_ref() {
                        if !next_depends.contains(replacement_task_id) {
                            next_depends.push(replacement_task_id.clone());
                        }
                    }
                    let mut update = empty_task_update();
                    update.depends_on_task_ids = Some(next_depends);
                    update.blocked_reason = Some(None);
                    update_task(db, dependent_id.as_str(), update).await?;
                    updated_task_ids.push(dependent_id);
                }
                operation_results.push(TaskPlanOperationResult {
                    kind,
                    task_id,
                    affected_count: updated_task_ids.len() as i64,
                    affected_task_ids: updated_task_ids,
                    replacement_task_id,
                });
            }
            _ => {
                return Err(format!("unsupported task plan operation: {}", kind));
            }
        }
    }

    let mut seen_updates = HashSet::new();
    for patch in req.updates {
        let task_id = patch.task_id.trim().to_string();
        if task_id.is_empty() {
            return Err("task_id 不能为空".to_string());
        }
        if !task_ids.contains(task_id.as_str()) {
            return Err(format!("任务 {} 不属于当前计划", task_id));
        }
        if !seen_updates.insert(task_id.clone()) {
            return Err(format!("任务 {} 在本次计划更新中重复出现", task_id));
        }

        if let Some(depends_on_task_ids) = patch.depends_on_task_ids.as_ref() {
            let normalized = normalize_string_list(depends_on_task_ids.clone());
            if normalized.iter().any(|id| id == &task_id) {
                return Err(format!("任务 {} 不能依赖自己", task_id));
            }
            if normalized.iter().any(|id| !task_ids.contains(id.as_str())) {
                return Err(format!("任务 {} 的前置依赖超出当前计划", task_id));
            }
        }
        if let Some(verification_of_task_ids) = patch.verification_of_task_ids.as_ref() {
            let normalized = normalize_string_list(verification_of_task_ids.clone());
            if normalized.iter().any(|id| id == &task_id) {
                return Err(format!("任务 {} 不能把自己作为验证对象", task_id));
            }
            if normalized.iter().any(|id| !task_ids.contains(id.as_str())) {
                return Err(format!("任务 {} 的验证对象超出当前计划", task_id));
            }
        }

        let mut update = empty_task_update();
        update.status = patch
            .status
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        update.queue_position = patch.queue_position;
        update.depends_on_task_ids = patch.depends_on_task_ids;
        update.verification_of_task_ids = patch.verification_of_task_ids;
        update.blocked_reason = patch.blocked_reason.map(|value| {
            value.and_then(|item| {
                let trimmed = item.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
        });
        update_task(db, task_id.as_str(), update).await?;
    }

    for (user_id, contact_agent_id, project_id) in scope_keys {
        refresh_blocked_scope_tasks(
            db,
            user_id.as_str(),
            contact_agent_id.as_str(),
            project_id.as_str(),
        )
        .await?;
    }

    let Some(plan) = get_task_plan(db, plan_id, &[]).await? else {
        return Ok(None);
    };
    Ok(Some(UpdateTaskPlanResponse {
        item: plan,
        operation_results,
    }))
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
    let queue_position = if task.queue_position > 0 {
        task.queue_position
    } else {
        next_queue_position(
            db,
            task.user_id.as_str(),
            task.contact_agent_id.as_str(),
            task.project_id.as_str(),
        )
        .await?
    };
    let next_status = if task.depends_on_task_ids.is_empty() {
        "pending_execute".to_string()
    } else {
        "blocked".to_string()
    };
    update_task(
        db,
        task_id,
        UpdateTaskRequest {
            title: None,
            content: None,
            priority: None,
            status: Some(next_status),
            task_ref: None,
            task_kind: None,
            depends_on_task_ids: None,
            verification_of_task_ids: None,
            acceptance_criteria: None,
            blocked_reason: Some(if task.depends_on_task_ids.is_empty() {
                None
            } else {
                Some("waiting_for_dependencies".to_string())
            }),
            confirm_note: note,
            execution_note: None,
            project_root: None,
            remote_connection_id: None,
            planned_builtin_mcp_ids: None,
            planned_context_assets: None,
            execution_result_contract: None,
            planning_snapshot: None,
            handoff_payload: None,
            model_config_id: None,
            queue_position: Some(queue_position),
            pause_reason: None,
            last_checkpoint_summary: Some(None),
            last_checkpoint_message_id: Some(None),
            resume_note: Some(None),
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
        if runtime
            .control_request
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        {
            return Ok(SchedulerDecision {
                decision: "pass".to_string(),
                task: None,
                scope_key: key,
            });
        }
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

    let paused_tasks = list_scope_tasks(db, user_id, contact_agent_id, project_id, &["paused"]).await?;
    if !paused_tasks.is_empty() {
        return Ok(SchedulerDecision {
            decision: "await_resume".to_string(),
            task: None,
            scope_key: key,
        });
    }

    refresh_blocked_scope_tasks(db, user_id, contact_agent_id, project_id).await?;

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

pub async fn request_pause_task(
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
    upsert_scope_runtime(
        db,
        task.scope_key.as_str(),
        task.user_id.as_str(),
        task.contact_agent_id.as_str(),
        task.project_id.as_str(),
        Some(task.id.as_str()),
        Some("pause".to_string()),
        Some(chrono::Utc::now().to_rfc3339()),
        reason.clone(),
        None,
        None,
    )
    .await?;
    Ok(Some(task))
}

pub async fn request_stop_task(
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
    upsert_scope_runtime(
        db,
        task.scope_key.as_str(),
        task.user_id.as_str(),
        task.contact_agent_id.as_str(),
        task.project_id.as_str(),
        Some(task.id.as_str()),
        Some("stop".to_string()),
        Some(chrono::Utc::now().to_rfc3339()),
        reason.clone(),
        None,
        None,
    )
    .await?;
    Ok(Some(task))
}

pub async fn ack_pause_task(
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
    let updated = update_task(
        db,
        task_id,
        UpdateTaskRequest {
            title: None,
            content: None,
            priority: None,
            status: Some("paused".to_string()),
            task_ref: None,
            task_kind: None,
            depends_on_task_ids: None,
            verification_of_task_ids: None,
            acceptance_criteria: None,
            blocked_reason: Some(None),
            confirm_note: None,
            execution_note: None,
            project_root: None,
            remote_connection_id: None,
            planned_builtin_mcp_ids: None,
            planned_context_assets: None,
            execution_result_contract: None,
            planning_snapshot: None,
            handoff_payload: Some(build_handoff_payload(
                &task,
                "checkpoint",
                checkpoint_summary.as_deref(),
                checkpoint_summary.as_deref(),
                None,
                checkpoint_message_id.as_deref(),
                pause_reason.as_deref(),
            )),
            model_config_id: None,
            queue_position: Some(task.queue_position),
            pause_reason: Some(pause_reason),
            last_checkpoint_summary: Some(checkpoint_summary),
            last_checkpoint_message_id: Some(checkpoint_message_id),
            resume_note: Some(None),
            result_summary: None,
            result_message_id: None,
            last_error: None,
        },
    )
    .await?;
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
    .await?;
    Ok(updated)
}

pub async fn ack_stop_task(
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
    let updated = update_task(
        db,
        task_id,
        UpdateTaskRequest {
            title: None,
            content: None,
            priority: None,
            status: Some("cancelled".to_string()),
            task_ref: None,
            task_kind: None,
            depends_on_task_ids: None,
            verification_of_task_ids: None,
            acceptance_criteria: None,
            blocked_reason: Some(None),
            confirm_note: None,
            execution_note: None,
            project_root: None,
            remote_connection_id: None,
            planned_builtin_mcp_ids: None,
            planned_context_assets: None,
            execution_result_contract: None,
            planning_snapshot: None,
            handoff_payload: Some(build_handoff_payload(
                &task,
                "cancelled",
                result_summary.as_deref().or(last_error.as_deref()),
                result_summary.as_deref(),
                result_message_id.as_deref(),
                None,
                last_error.as_deref(),
            )),
            model_config_id: None,
            queue_position: Some(task.queue_position),
            pause_reason: Some(None),
            last_checkpoint_summary: Some(None),
            last_checkpoint_message_id: Some(None),
            resume_note: Some(None),
            result_summary: Some(result_summary),
            result_message_id: Some(result_message_id),
            last_error: Some(last_error),
        },
    )
    .await?;
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
    .await?;
    Ok(updated)
}

pub async fn resume_task(
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
    let queue_position = if task.queue_position > 0 {
        task.queue_position
    } else {
        next_queue_position(
            db,
            task.user_id.as_str(),
            task.contact_agent_id.as_str(),
            task.project_id.as_str(),
        )
        .await?
    };
    let updated = update_task(
        db,
        task_id,
        UpdateTaskRequest {
            title: None,
            content: None,
            priority: None,
            status: Some("pending_execute".to_string()),
            task_ref: None,
            task_kind: None,
            depends_on_task_ids: None,
            verification_of_task_ids: None,
            acceptance_criteria: None,
            blocked_reason: Some(None),
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
            queue_position: Some(queue_position),
            pause_reason: Some(None),
            last_checkpoint_summary: None,
            last_checkpoint_message_id: None,
            resume_note: Some(resume_note),
            result_summary: None,
            result_message_id: None,
            last_error: None,
        },
    )
    .await?;
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

pub async fn retry_task(
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
    let queue_position = if task.queue_position > 0 {
        task.queue_position
    } else {
        next_queue_position(
            db,
            task.user_id.as_str(),
            task.contact_agent_id.as_str(),
            task.project_id.as_str(),
        )
        .await?
    };
    let next_status = if task.depends_on_task_ids.is_empty() {
        "pending_execute".to_string()
    } else {
        "blocked".to_string()
    };

    let updated = ContactTask {
        id: task.id.clone(),
        user_id: task.user_id.clone(),
        contact_agent_id: task.contact_agent_id.clone(),
        project_id: task.project_id.clone(),
        scope_key: task.scope_key.clone(),
        task_plan_id: task.task_plan_id.clone(),
        task_ref: task.task_ref.clone(),
        task_kind: task.task_kind.clone(),
        depends_on_task_ids: task.depends_on_task_ids.clone(),
        verification_of_task_ids: task.verification_of_task_ids.clone(),
        acceptance_criteria: task.acceptance_criteria.clone(),
        blocked_reason: if task.depends_on_task_ids.is_empty() {
            None
        } else {
            Some("waiting_for_dependencies".to_string())
        },
        project_root: task.project_root.clone(),
        remote_connection_id: task.remote_connection_id.clone(),
        session_id: task.session_id.clone(),
        conversation_turn_id: task.conversation_turn_id.clone(),
        source_message_id: task.source_message_id.clone(),
        model_config_id: task.model_config_id.clone(),
        title: task.title.clone(),
        content: task.content.clone(),
        priority: task.priority.clone(),
        priority_rank: task.priority_rank,
        queue_position,
        status: next_status,
        confirm_note: task.confirm_note.clone(),
        execution_note: task.execution_note.clone(),
        planned_builtin_mcp_ids: task.planned_builtin_mcp_ids.clone(),
        planned_context_assets: task.planned_context_assets.clone(),
        execution_result_contract: task.execution_result_contract.clone(),
        planning_snapshot: task.planning_snapshot.clone(),
        handoff_payload: None,
        created_by: task.created_by.clone(),
        created_at: task.created_at.clone(),
        updated_at: now.clone(),
        confirmed_at: task.confirmed_at.clone().or_else(|| Some(now.clone())),
        started_at: None,
        paused_at: None,
        pause_reason: None,
        last_checkpoint_summary: None,
        last_checkpoint_message_id: None,
        resume_note: normalize_optional_text(req.note),
        finished_at: None,
        last_error: None,
        result_summary: None,
        result_message_id: None,
    };

    tasks(db)
        .replace_one(doc! {"id": task_id}, updated.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(updated))
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

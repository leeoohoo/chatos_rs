use std::collections::{BTreeMap, HashSet};

use crate::db::Db;
use crate::models::{TaskPlanOperationResult, UpdateTaskPlanRequest, UpdateTaskPlanResponse};

use super::support::{
    apply_status_and_blocked_reason, build_direct_dependents_map, collect_descendant_ids, empty_task_update,
    is_terminal_plan_status, list_plan_tasks, normalize_string_list, sort_plan_tasks,
};
use super::{get_task_plan, refresh_blocked_scope_tasks, update_task};

pub(super) async fn update_task_plan(
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
                    apply_status_and_blocked_reason(&mut update, "skipped", None);
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

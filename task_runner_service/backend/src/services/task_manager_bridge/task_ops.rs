// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{
    apply_manager_patch, shared_outcome_items_into_tool_state, task_belongs_to_context,
    task_manager_status_from_task_status, task_priority_from_manager_label,
    task_status_from_manager_status,
};
use super::*;
use crate::models::{TaskClosureState, TaskListFilters, TaskManagerScope};
use crate::services::task_manager_lifecycle::{
    apply_task_closure, effective_task_manager_scope, load_task_session_snapshot,
    task_closure_is_terminal, task_is_in_session,
};
use crate::services::unfinished_subtasks_error;
use crate::store::AppStore;

const MAX_RUN_CHECKLIST_TASKS: usize = 32;
const TASK_RELEVANCE_MARKERS: &[&str] = &[
    "oms",
    "sales_order",
    "salesorder",
    "订单",
    "客户",
    "图号",
    "材质",
    "审核",
    "内联",
    "建档",
    "mdm",
    "主数据",
    "postgresql",
    "postgres",
    "库存",
    "inventory",
    "bom",
    "planning",
    "设备",
    "推荐",
    "device",
    "生产",
    "采购",
    "procurement",
    "quality",
    "qms",
    "质检",
    "t09",
    "iam",
    "workflow",
    "原型",
    "导航",
    "小程序",
    "bi",
    "uat",
    "迁移",
    "上线",
    "报表",
];

impl TaskService {
    pub(super) async fn create_followup_task_for_tool(
        &self,
        root_task_id: &str,
        run_id: &str,
        draft: SharedTaskDraft,
    ) -> Result<TaskRecord, String> {
        validate_required("title", &draft.title)?;
        let Some(parent) = self.store.get_task(root_task_id).await? else {
            warn!(
                root_task_id,
                source_run_id = run_id,
                draft_title = draft.title.as_str(),
                "task manager could not find root task for follow-up task creation"
            );
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        let parent = save_task_if_tenant_aligned(&self.store, parent).await?;
        if parent.status == TaskStatus::Succeeded {
            return Err(format!(
                "父任务「{}」已经成功，不能再新增子任务。",
                parent.title.trim()
            ));
        }
        let scope = task_manager_scope_from_label(draft.scope.as_str())?;
        let required_for_parent_completion =
            scope == TaskManagerScope::RunChecklist && draft.required_for_parent_completion;
        let title = draft.title.trim().to_string();
        let description = normalized_optional(Some(draft.details.clone()));
        let objective = description.clone().unwrap_or_else(|| title.clone());
        if scope == TaskManagerScope::RunChecklist && draft.required_for_parent_completion {
            validate_run_checklist_task_relevance(&parent, title.as_str(), objective.as_str())?;
        }
        let idempotency_key = normalized_optional(draft.idempotency_key.clone())
            .unwrap_or_else(|| task_semantic_fingerprint(title.as_str(), objective.as_str()));
        let current_session_tasks = self
            .store
            .list_tasks_filtered(&TaskListFilters {
                parent_task_id: Some(parent.id.clone()),
                ..TaskListFilters::default()
            })
            .await?
            .into_iter()
            .filter(|task| task_is_in_session(task, run_id))
            .collect::<Vec<_>>();
        if let Some(existing) = current_session_tasks.iter().find(|task| {
            task.task_tool_state.idempotency_key.as_deref() == Some(idempotency_key.as_str())
                || task_semantic_fingerprint(task.title.as_str(), task.objective.as_str())
                    == idempotency_key
        }) {
            info!(
                root_task_id,
                source_run_id = run_id,
                existing_task_id = existing.id.as_str(),
                existing_task_title = existing.title.as_str(),
                "task manager reused idempotent current-session task"
            );
            return self.hydrate_task_prerequisites(existing.clone()).await;
        }
        if scope == TaskManagerScope::RunChecklist
            && current_session_tasks
                .iter()
                .filter(|task| effective_task_manager_scope(task) == TaskManagerScope::RunChecklist)
                .count()
                >= MAX_RUN_CHECKLIST_TASKS
        {
            return Err(format!(
                "当前运行最多允许创建 {MAX_RUN_CHECKLIST_TASKS} 个运行内清单任务，请复用或收口已有任务"
            ));
        }
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let prerequisite_task_ids = tool_prerequisite_task_ids(&draft);
        self.validate_tool_prerequisite_task_ids(
            root_task_id,
            &id,
            &prerequisite_task_ids,
            parent.project_id.as_str(),
        )
        .await?;
        let result_summary = normalized_optional(Some(draft.outcome_summary));
        let status = task_status_from_manager_status(draft.status.as_str());
        let mut task_tool_state = TaskToolState {
            due_at: normalized_optional_nested(draft.due_at),
            outcome_items: shared_outcome_items_into_tool_state(draft.outcome_items),
            resume_hint: normalized_optional(Some(draft.resume_hint)),
            blocker_reason: normalized_optional(Some(draft.blocker_reason)),
            blocker_needs: normalize_strings(draft.blocker_needs),
            blocker_kind: normalized_optional(Some(draft.blocker_kind)),
            completed_at: None,
            last_outcome_at: None,
            manager_scope: Some(scope),
            task_session_id: Some(run_id.to_string()),
            required_for_parent_completion: Some(required_for_parent_completion),
            closure_state: Some(if task_manager_status_from_task_status(status) == "done" {
                TaskClosureState::Satisfied
            } else {
                TaskClosureState::Open
            }),
            closure_reason: None,
            superseded_by_run_id: None,
            idempotency_key: Some(idempotency_key),
            lifecycle_updated_at: Some(now.clone()),
            ..TaskToolState::default()
        };
        if result_summary.is_some() || !task_tool_state.outcome_items.is_empty() {
            task_tool_state.last_outcome_at = Some(now.clone());
        }
        if task_manager_status_from_task_status(status) == "done" {
            task_tool_state.blocker_reason = None;
            task_tool_state.blocker_needs.clear();
            task_tool_state.blocker_kind = None;
            task_tool_state.completed_at = Some(now.clone());
        }

        let input_payload = None;
        let mcp_config = tool_subtask_mcp_config(&parent);

        let task = TaskRecord {
            id: id.clone(),
            title,
            description,
            objective,
            input_payload,
            status,
            priority: task_priority_from_manager_label(draft.priority.as_str()),
            tags: normalize_strings(draft.tags),
            default_model_config_id: None,
            memory_thread_id: format!("task-subtask-{id}"),
            tenant_id: parent.tenant_id.clone(),
            subject_id: parent.subject_id.clone(),
            project_id: parent.project_id.clone(),
            task_profile: parent.task_profile.clone(),
            creator_user_id: parent.creator_user_id.clone(),
            creator_username: parent.creator_username.clone(),
            creator_display_name: parent.creator_display_name.clone(),
            owner_user_id: parent.owner_user_id.clone(),
            owner_username: parent.owner_username.clone(),
            owner_display_name: parent.owner_display_name.clone(),
            result_summary,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: Some(parent.id.clone()),
            source_run_id: Some(run_id.to_string()),
            source_session_id: parent.source_session_id.clone(),
            source_turn_id: parent.source_turn_id.clone(),
            source_user_message_id: parent.source_user_message_id.clone(),
            prerequisite_task_ids: prerequisite_task_ids.clone(),
            task_tool_state,
            plugin_config: parent.plugin_config.clone(),
            mcp_config,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        let saved = self.store.save_task(task).await?;
        if !prerequisite_task_ids.is_empty() {
            self.store
                .set_task_prerequisites(&id, prerequisite_task_ids)
                .await?;
        }
        let saved = self.hydrate_task_prerequisites(saved).await?;
        info!(
            root_task_id,
            source_run_id = run_id,
            created_task_id = saved.id.as_str(),
            created_task_title = saved.title.as_str(),
            created_task_status = saved.status.status_string(),
            "task manager auto-created follow-up task during task run"
        );
        Ok(saved)
    }

    pub(super) async fn list_tool_tasks(
        &self,
        root_task_id: &str,
        source_run_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<TaskRecord>, String> {
        if self.store.get_task(root_task_id).await?.is_none() {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }
        let mut tasks = self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .filter(|task| task_belongs_to_context(task, root_task_id))
            .collect::<Vec<_>>();
        if let Some(run_id) = source_run_id {
            tasks.retain(|task| task_is_in_session(task, run_id));
        }
        if !include_done {
            tasks.retain(|task| !task_closure_is_terminal(task));
        }
        tasks.sort_by(|left, right| {
            if left.id == root_task_id && right.id != root_task_id {
                std::cmp::Ordering::Less
            } else if right.id == root_task_id && left.id != root_task_id {
                std::cmp::Ordering::Greater
            } else {
                right.updated_at.cmp(&left.updated_at)
            }
        });
        tasks.truncate(limit);
        self.hydrate_tasks_prerequisites(tasks).await
    }

    pub(super) async fn update_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<TaskRecord, String> {
        self.update_task_from_tool_in_session(root_task_id, None, task_id, patch)
            .await
    }

    pub(super) async fn update_task_from_tool_in_session(
        &self,
        root_task_id: &str,
        run_id: Option<&str>,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }
        ensure_task_is_mutable_in_session(&task, root_task_id, run_id)?;

        let now = now_rfc3339();
        let previous_status = task.status;
        let patch = neutralize_internal_tool_availability_blocker_patch(patch);
        apply_manager_patch(&mut task, patch, false, now.as_str())?;
        if task.parent_task_id.is_none()
            && previous_status == TaskStatus::Succeeded
            && task.status != TaskStatus::Succeeded
        {
            return Err(format!(
                "任务「{}」已经成功，不能再改为 {}。",
                task.title.trim(),
                task.status.status_string()
            ));
        }
        if task.status != previous_status {
            ensure_subtask_can_be_marked_unfinished(&self.store, &task, task.status).await?;
            if task.parent_task_id.is_some() {
                if task.status == TaskStatus::Succeeded {
                    apply_task_closure(&mut task, TaskClosureState::Satisfied, None, now.as_str())?;
                } else {
                    task.task_tool_state.closure_state = Some(TaskClosureState::Open);
                    task.task_tool_state.closure_reason = None;
                    task.task_tool_state.completed_at = None;
                    task.task_tool_state.lifecycle_updated_at = Some(now.clone());
                    if let Some(run_id) = run_id {
                        task.task_tool_state.task_session_id = Some(run_id.to_string());
                    }
                }
            }
        }
        if task.status == TaskStatus::Succeeded {
            if task.parent_task_id.is_none() {
                ensure_root_task_session_can_complete(&self.store, &task, run_id).await?;
            } else {
                ensure_task_has_no_unfinished_subtasks(&self.store, &task).await?;
            }
        }
        align_task_tenant_to_owner(&mut task);
        task.updated_at = now;
        if task.parent_task_id.is_none() {
            self.ensure_task_thread(&task).await?;
        }
        self.store.save_task(task).await
    }

    pub(super) async fn complete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<TaskRecord, String> {
        self.complete_task_from_tool_in_session(root_task_id, None, task_id, patch)
            .await
    }

    pub(super) async fn complete_task_from_tool_in_session(
        &self,
        root_task_id: &str,
        run_id: Option<&str>,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }
        ensure_task_is_mutable_in_session(&task, root_task_id, run_id)?;

        let now = now_rfc3339();
        if let Some(patch) = patch {
            apply_manager_patch(&mut task, patch, true, now.as_str())?;
        } else {
            task.status = TaskStatus::Succeeded;
            task.task_tool_state.blocker_reason = None;
            task.task_tool_state.blocker_needs.clear();
            task.task_tool_state.blocker_kind = None;
            task.task_tool_state.completed_at = Some(now.clone());
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        task.status = TaskStatus::Succeeded;
        if task.parent_task_id.is_none() {
            ensure_root_task_session_can_complete(&self.store, &task, run_id).await?;
        } else {
            ensure_task_has_no_unfinished_subtasks(&self.store, &task).await?;
            apply_task_closure(&mut task, TaskClosureState::Satisfied, None, now.as_str())?;
        }
        if task.task_tool_state.completed_at.is_none() {
            task.task_tool_state.completed_at = Some(now.clone());
        }
        if task.task_tool_state.last_outcome_at.is_none() {
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        align_task_tenant_to_owner(&mut task);
        task.updated_at = now;
        if task.parent_task_id.is_none() {
            self.ensure_task_thread(&task).await?;
        }
        self.store.save_task(task).await
    }

    pub(super) async fn delete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        self.delete_task_from_tool_in_session(root_task_id, None, task_id)
            .await
    }

    pub(super) async fn delete_task_from_tool_in_session(
        &self,
        root_task_id: &str,
        run_id: Option<&str>,
        task_id: &str,
    ) -> Result<bool, String> {
        if task_id == root_task_id {
            return Err("不能删除当前正在执行的根任务".to_string());
        }
        let Some(task) = self.store.get_task(task_id).await? else {
            return Ok(false);
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Ok(false);
        }
        ensure_task_is_mutable_in_session(&task, root_task_id, run_id)?;
        if self.store.has_active_run_for_task(task_id).await? {
            return Err("任务仍有运行中的执行记录，暂时不能删除".to_string());
        }
        self.store.delete_task(task_id).await
    }

    pub(super) async fn reconcile_tool_tasks(
        &self,
        root_task_id: &str,
        run_id: &str,
        decisions: Vec<SharedTaskClosureDecision>,
    ) -> Result<Value, String> {
        let mut prepared = Vec::with_capacity(decisions.len());
        let mut rejected_internal_tool_blockers = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for decision in decisions {
            validate_required("task_id", &decision.task_id)?;
            if !seen.insert(decision.task_id.clone()) {
                return Err(format!(
                    "同一个任务不能在一次收口请求中重复出现: {}",
                    decision.task_id
                ));
            }
            let Some(task) = self.store.get_task(decision.task_id.as_str()).await? else {
                return Err(TASK_NOT_FOUND_ERR.to_string());
            };
            if task.id == root_task_id || !task_belongs_to_context(&task, root_task_id) {
                return Err(TASK_NOT_FOUND_ERR.to_string());
            }
            ensure_task_is_mutable_in_session(&task, root_task_id, Some(run_id))?;
            let closure_state = task_closure_state_from_label(decision.closure_state.as_str())?;
            if closure_state != TaskClosureState::Satisfied && decision.reason.trim().is_empty() {
                return Err(format!(
                    "任务 {} 以 {} 收口时必须提供 reason",
                    decision.task_id, decision.closure_state
                ));
            }
            if closure_state == TaskClosureState::BlockedTerminal
                && closure_decision_looks_like_internal_tool_availability_blocker(&decision)
            {
                rejected_internal_tool_blockers.push(json!({
                    "task_id": decision.task_id,
                    "title": task.title,
                    "closure_state": decision.closure_state,
                    "reason": "internal_tool_availability_blocker",
                    "message": "不能仅因为本轮仓库读取或终端工具未暴露、被临时限制、不可用而将任务标记为 blocked_terminal；这类情况属于执行过程保护或运行配置问题，不是业务终态阻塞。请基于已取得的真实证据继续实现/验证，或说明具体缺失的外部 artifact、服务、凭据或人工决策。",
                }));
                continue;
            }
            prepared.push((task, decision, closure_state));
        }

        let mut reconciled = Vec::with_capacity(prepared.len());
        for (mut task, decision, closure_state) in prepared {
            let now = now_rfc3339();
            if let Some(outcome_summary) =
                normalized_optional(Some(decision.outcome_summary.clone()))
            {
                task.result_summary = Some(outcome_summary);
                task.task_tool_state.last_outcome_at = Some(now.clone());
            }
            if !decision.outcome_items.is_empty() {
                task.task_tool_state.outcome_items =
                    shared_outcome_items_into_tool_state(decision.outcome_items);
                task.task_tool_state.last_outcome_at = Some(now.clone());
            }
            if let Some(resume_hint) = normalized_optional(Some(decision.resume_hint)) {
                task.task_tool_state.resume_hint = Some(resume_hint);
            }
            if closure_state == TaskClosureState::BlockedTerminal {
                task.task_tool_state.blocker_reason =
                    normalized_optional(Some(decision.reason.clone()));
            }
            apply_task_closure(
                &mut task,
                closure_state,
                Some(decision.reason),
                now.as_str(),
            )?;
            reconciled.push(self.store.save_task(task).await?);
        }

        let snapshot = load_task_session_snapshot(&self.store, root_task_id, run_id).await?;
        let rejected_count = rejected_internal_tool_blockers.len();
        let has_rejected = rejected_count > 0;
        Ok(json!({
            "reconciled": !has_rejected,
            "reconciled_count": reconciled.len(),
            "rejected_count": rejected_count,
            "rejected": rejected_internal_tool_blockers,
            "guidance": if has_rejected {
                json!("blocked_terminal must describe a real external blocker such as a missing artifact, service, credential, or user decision. Temporary absence or rate-limiting of read/terminal tools is not a business terminal blocker; keep tasks open, continue from available evidence, or let the completion gate close the run.")
            } else {
                Value::Null
            },
            "tasks": reconciled.iter().map(super::support::task_to_manager_value).collect::<Vec<_>>(),
            "session": task_session_snapshot_value(&snapshot),
        }))
    }

    pub(super) async fn finalize_tool_task_session(
        &self,
        root_task_id: &str,
        run_id: &str,
    ) -> Result<Value, String> {
        if self.store.get_task(root_task_id).await?.is_none() {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }
        let snapshot = load_task_session_snapshot(&self.store, root_task_id, run_id).await?;
        Ok(json!({
            "finalized": snapshot.open_required.is_empty(),
            "can_parent_succeed": snapshot.open_required.is_empty() && snapshot.terminal_blocked.is_empty(),
            "parent_should_block": !snapshot.terminal_blocked.is_empty(),
            "session": task_session_snapshot_value(&snapshot),
        }))
    }

    async fn validate_tool_prerequisite_task_ids(
        &self,
        root_task_id: &str,
        task_id: &str,
        prerequisite_task_ids: &[String],
        expected_project_id: &str,
    ) -> Result<(), String> {
        self.validate_task_prerequisites_for_project(
            task_id,
            prerequisite_task_ids,
            None,
            Some(expected_project_id),
        )
        .await?;
        for prerequisite_task_id in prerequisite_task_ids {
            if prerequisite_task_id == root_task_id {
                return Err("前置任务不能是当前正在执行的父任务".to_string());
            }
            let Some(prerequisite) = self.store.get_task(prerequisite_task_id).await? else {
                return Err(format!("前置任务不存在: {prerequisite_task_id}"));
            };
            if !task_belongs_to_context(&prerequisite, root_task_id) {
                return Err(format!(
                    "前置任务不属于当前内部任务上下文: {prerequisite_task_id}"
                ));
            }
        }
        Ok(())
    }
}

fn validate_run_checklist_task_relevance(
    parent: &TaskRecord,
    title: &str,
    objective: &str,
) -> Result<(), String> {
    let parent_text = format!(
        "{}\n{}\n{}",
        parent.title,
        parent.description.as_deref().unwrap_or_default(),
        parent.objective,
    );
    let parent_markers = task_relevance_markers(parent_text.as_str());
    let child_text = format!("{title}\n{objective}");
    let child_markers = task_relevance_markers(child_text.as_str());
    if parent_markers.is_empty()
        || child_markers.is_empty()
        || child_markers
            .iter()
            .any(|marker| parent_markers.contains(marker))
    {
        return Ok(());
    }
    Err(format!(
        "运行内必需清单「{}」偏离父任务「{}」的当前目标；请不要把其他需求、页面、T09、库存、设备推荐等无关工作加入本轮必需清单。",
        title.trim(),
        parent.title.trim(),
    ))
}

fn task_relevance_markers(text: &str) -> Vec<&'static str> {
    let normalized = text.to_ascii_lowercase();
    TASK_RELEVANCE_MARKERS
        .iter()
        .copied()
        .filter(|marker| normalized.contains(marker))
        .collect()
}

fn tool_prerequisite_task_ids(draft: &SharedTaskDraft) -> Vec<String> {
    let mut ids = draft.prerequisite_task_ids.clone();
    if let Some(id) = draft.prerequisite_task_id.clone() {
        ids.push(id);
    }
    normalize_prerequisite_task_ids(ids)
}

fn task_manager_scope_from_label(value: &str) -> Result<TaskManagerScope, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "run_checklist" => Ok(TaskManagerScope::RunChecklist),
        "durable_followup" => Ok(TaskManagerScope::DurableFollowup),
        other => Err(format!("unsupported task scope: {other}")),
    }
}

fn task_closure_state_from_label(value: &str) -> Result<TaskClosureState, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "satisfied" => Ok(TaskClosureState::Satisfied),
        "blocked_terminal" => Ok(TaskClosureState::BlockedTerminal),
        "cancelled" => Ok(TaskClosureState::Cancelled),
        "superseded" => Ok(TaskClosureState::Superseded),
        "waived" => Ok(TaskClosureState::Waived),
        other => Err(format!("unsupported closure_state: {other}")),
    }
}

fn closure_decision_looks_like_internal_tool_availability_blocker(
    decision: &SharedTaskClosureDecision,
) -> bool {
    let mut text = String::new();
    text.push_str(decision.reason.as_str());
    text.push('\n');
    text.push_str(decision.outcome_summary.as_str());
    text.push('\n');
    text.push_str(decision.resume_hint.as_str());
    for item in &decision.outcome_items {
        text.push('\n');
        text.push_str(item.text.as_str());
    }
    text_looks_like_internal_tool_availability_blocker(text.as_str())
}

fn neutralize_internal_tool_availability_blocker_patch(
    mut patch: SharedTaskUpdatePatch,
) -> SharedTaskUpdatePatch {
    let requests_blocked = patch
        .status
        .as_deref()
        .is_some_and(|status| task_status_from_manager_status(status) == TaskStatus::Blocked);
    if !requests_blocked || !update_patch_looks_like_internal_tool_availability_blocker(&patch) {
        return patch;
    }

    patch.status = None;
    patch.blocker_reason = None;
    patch.blocker_needs = None;
    patch.blocker_kind = None;
    patch.outcome_items = None;
    patch.outcome_summary = Some(
        "任务仍未完成；不能仅因本轮仓库读取或终端工具临时不可用而标记为业务阻塞。".to_string(),
    );
    patch.resume_hint = Some(
        "继续基于已取得的真实证据实现/验证；如确有外部 artifact、服务、凭据或人工决策缺失，请说明具体对象。"
            .to_string(),
    );
    patch
}

fn update_patch_looks_like_internal_tool_availability_blocker(
    patch: &SharedTaskUpdatePatch,
) -> bool {
    let mut text = String::new();
    if let Some(value) = &patch.outcome_summary {
        text.push_str(value);
    }
    if let Some(value) = &patch.resume_hint {
        text.push('\n');
        text.push_str(value);
    }
    if let Some(value) = &patch.blocker_reason {
        text.push('\n');
        text.push_str(value);
    }
    if let Some(values) = &patch.blocker_needs {
        for value in values {
            text.push('\n');
            text.push_str(value);
        }
    }
    if let Some(values) = &patch.outcome_items {
        for value in values {
            text.push('\n');
            text.push_str(value.text.as_str());
        }
    }
    text_looks_like_internal_tool_availability_blocker(text.as_str())
}

fn text_looks_like_internal_tool_availability_blocker(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    let mentions_tool_surface = [
        "仓库读取",
        "读取能力",
        "文件读取",
        "源码读取",
        "未读取",
        "读取工具",
        "读文件工具",
        "代码读取",
        "真实文件",
        "schema",
        "测试执行能力",
        "执行能力",
        "验证能力",
        "运行测试",
        "终端",
        "执行工具",
        "工具清单",
        "read tool",
        "read-file",
        "read_file",
        "terminal",
        "tool surface",
        "tool list",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    let mentions_availability = [
        "未暴露",
        "未能",
        "没有",
        "缺少",
        "无法",
        "不能",
        "不可用",
        "临时限制",
        "临时不可用",
        "被限制",
        "not exposed",
        "unavailable",
        "temporarily unavailable",
        "disabled",
        "rate-limited",
        "rate limited",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    mentions_tool_surface && mentions_availability
}

fn task_semantic_fingerprint(title: &str, objective: &str) -> String {
    fn normalized(value: &str) -> String {
        value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    }
    format!("{}\n{}", normalized(title), normalized(objective))
}

fn ensure_task_is_mutable_in_session(
    task: &TaskRecord,
    root_task_id: &str,
    run_id: Option<&str>,
) -> Result<(), String> {
    if task.id == root_task_id || run_id.is_none() {
        return Ok(());
    }
    if task_is_in_session(task, run_id.unwrap_or_default()) {
        Ok(())
    } else {
        Err(format!(
            "任务 {} 不属于当前运行的 Task Manager 会话，不能修改历史运行任务",
            task.id
        ))
    }
}

async fn ensure_root_task_session_can_complete(
    store: &AppStore,
    task: &TaskRecord,
    run_id: Option<&str>,
) -> Result<(), String> {
    let Some(run_id) = run_id else {
        return ensure_task_has_no_unfinished_subtasks(store, task).await;
    };
    let snapshot = load_task_session_snapshot(store, task.id.as_str(), run_id).await?;
    if !snapshot.terminal_blocked.is_empty() {
        return Err(format!(
            "当前运行有 {} 个已确认的终态阻塞任务，父任务应进入 blocked，而不是标记成功",
            snapshot.terminal_blocked.len()
        ));
    }
    if snapshot.open_required.is_empty() {
        Ok(())
    } else {
        Err(unfinished_subtasks_error(task, &snapshot.open_required))
    }
}

fn task_session_snapshot_value(
    snapshot: &crate::services::task_manager_lifecycle::TaskSessionSnapshot,
) -> Value {
    json!({
        "entry_count": snapshot.entries.len(),
        "open_required_count": snapshot.open_required.len(),
        "open_optional_count": snapshot.open_optional.len(),
        "terminal_blocked_count": snapshot.terminal_blocked.len(),
        "open_required_tasks": snapshot.open_required.iter().map(super::support::task_to_manager_value).collect::<Vec<_>>(),
        "open_optional_tasks": snapshot.open_optional.iter().map(super::support::task_to_manager_value).collect::<Vec<_>>(),
        "terminal_blocked_tasks": snapshot.terminal_blocked.iter().map(super::support::task_to_manager_value).collect::<Vec<_>>(),
    })
}

fn disabled_tool_subtask_mcp_config() -> TaskMcpConfig {
    TaskMcpConfig {
        enabled: false,
        enabled_builtin_kinds: Vec::new(),
        external_mcp_config_ids: Vec::new(),
        workspace_dir: None,
        default_remote_server_id: None,
        ..TaskMcpConfig::default()
    }
}

fn tool_subtask_mcp_config(parent: &TaskRecord) -> TaskMcpConfig {
    if !parent.mcp_config.enabled {
        return disabled_tool_subtask_mcp_config();
    }
    let mut config = parent.mcp_config.clone();
    config.default_remote_server_id = None;
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, StoreMode};
    use crate::models::{CancelTaskRequest, CreateTaskRequest, TaskRunStatus, UpdateTaskRequest};
    use crate::services::task_manager_lifecycle::{
        block_open_required_task_session_entries, effective_task_closure_state,
        effective_task_required_for_parent_completion, finalize_task_session_entries,
        load_task_session_snapshot,
    };
    use crate::store::AppStore;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://task-manager-bridge-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
            chatos_internal_api_secret: None,
            local_connector_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5000),
        }
    }

    async fn test_service() -> TaskService {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        TaskService::new(config, store)
    }

    async fn create_task(service: &TaskService, title: &str, status: TaskStatus) -> TaskRecord {
        service
            .create_task(
                CreateTaskRequest {
                    title: title.to_string(),
                    description: None,
                    objective: format!("do {title}"),
                    input_payload: None,
                    status: Some(status),
                    priority: None,
                    tags: None,
                    default_model_config_id: None,
                    project_id: None,
                    task_profile: None,
                    tenant_id: None,
                    subject_id: None,
                    schedule: None,
                    plugin_config: Default::default(),
                    mcp_config: None,
                    prerequisite_task_ids: None,
                },
                None,
                None,
            )
            .await
            .expect("create task")
    }

    fn task_draft(title: &str, status: &str) -> SharedTaskDraft {
        SharedTaskDraft {
            title: title.to_string(),
            details: String::new(),
            priority: "medium".to_string(),
            status: status.to_string(),
            tags: Vec::new(),
            prerequisite_task_id: None,
            prerequisite_task_ids: Vec::new(),
            due_at: None,
            outcome_summary: String::new(),
            outcome_items: Vec::new(),
            resume_hint: String::new(),
            blocker_reason: String::new(),
            blocker_needs: Vec::new(),
            blocker_kind: String::new(),
            scope: "run_checklist".to_string(),
            required_for_parent_completion: true,
            idempotency_key: None,
        }
    }

    #[tokio::test]
    async fn create_followup_task_rejects_succeeded_parent() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Succeeded).await;

        let err = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", task_draft("child", "todo"))
            .await
            .expect_err("completed parent should not accept new subtasks");

        assert!(err.contains("不能再新增子任务"));
    }

    #[tokio::test]
    async fn run_checklist_rejects_required_tasks_from_unrelated_domain() {
        let service = test_service().await;
        let parent = create_task(
            &service,
            "实现 OMS 订单录入、审核与内联建档流程",
            TaskStatus::Ready,
        )
        .await;

        for title in [
            "核对 T09 需求与仓库现状",
            "补齐库存校验页实现并验证",
            "实现 Planning 设备推荐编排",
        ] {
            let err = service
                .create_followup_task_for_tool(
                    parent.id.as_str(),
                    "run-1",
                    task_draft(title, "todo"),
                )
                .await
                .expect_err("unrelated checklist task must be rejected");

            assert!(err.contains("偏离父任务"), "{err}");
        }
    }

    #[tokio::test]
    async fn run_checklist_allows_generic_or_overlapping_required_tasks() {
        let service = test_service().await;
        let parent = create_task(
            &service,
            "实现 OMS 订单录入、审核与内联建档流程",
            TaskStatus::Ready,
        )
        .await;

        let generic = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-1",
                task_draft("执行验证并整理证据", "todo"),
            )
            .await
            .expect("generic verification checklist should be allowed");
        assert_eq!(generic.title, "执行验证并整理证据");

        let overlapping = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-1",
                task_draft("实现 OMS 订单审核 API", "todo"),
            )
            .await
            .expect("same-domain checklist should be allowed");
        assert_eq!(overlapping.title, "实现 OMS 订单审核 API");
    }

    #[tokio::test]
    async fn done_followup_task_clears_blocker_metadata() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let mut draft = task_draft("child", "done");
        draft.blocker_reason = "waiting".to_string();
        draft.blocker_needs = vec!["input".to_string()];
        draft.blocker_kind = "unknown".to_string();

        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", draft)
            .await
            .expect("create done child");

        assert_eq!(child.status, TaskStatus::Succeeded);
        assert_eq!(child.task_tool_state.blocker_reason, None);
        assert!(child.task_tool_state.blocker_needs.is_empty());
        assert_eq!(child.task_tool_state.blocker_kind, None);
    }

    #[tokio::test]
    async fn followup_task_inherits_requested_mcp_without_provider_route() {
        let service = test_service().await;
        let mut parent = create_task(&service, "parent", TaskStatus::Ready).await;
        parent.project_id = "project-local".to_string();
        parent.owner_user_id = Some("owner-1".to_string());
        parent.mcp_config = TaskMcpConfig {
            enabled: true,
            enabled_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "TaskManager".to_string(),
            ],
            ..TaskMcpConfig::default()
        };
        let parent = service.store.save_task(parent).await.expect("save parent");

        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", task_draft("child", "todo"))
            .await
            .expect("create child");

        assert!(child.input_payload.is_none());
        assert!(child.mcp_config.enabled);
        assert!(child
            .mcp_config
            .enabled_builtin_kinds
            .iter()
            .any(|kind| kind == "CodeMaintainerRead"));
        assert!(child.mcp_config.ephemeral_http_servers.is_empty());
    }

    #[tokio::test]
    async fn completing_task_clears_blocker_metadata() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let mut draft = task_draft("child", "blocked");
        draft.blocker_reason = "waiting".to_string();
        draft.blocker_needs = vec!["input".to_string()];
        draft.blocker_kind = "unknown".to_string();
        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", draft)
            .await
            .expect("create blocked child");

        let completed = service
            .complete_task_from_tool(parent.id.as_str(), child.id.as_str(), None)
            .await
            .expect("complete child");

        assert_eq!(completed.status, TaskStatus::Succeeded);
        assert_eq!(completed.task_tool_state.blocker_reason, None);
        assert!(completed.task_tool_state.blocker_needs.is_empty());
        assert_eq!(completed.task_tool_state.blocker_kind, None);
    }

    #[tokio::test]
    async fn update_task_soft_rejects_internal_tool_availability_blocker_status() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", task_draft("child", "todo"))
            .await
            .expect("create child");

        let updated = service
            .update_task_from_tool_in_session(
                parent.id.as_str(),
                Some("run-1"),
                child.id.as_str(),
                SharedTaskUpdatePatch {
                    status: Some("blocked".to_string()),
                    outcome_summary: Some(
                        "当前会话未暴露仓库读取或终端执行工具，无法继续核对真实代码。".to_string(),
                    ),
                    blocker_reason: Some(
                        "缺少源码读取与测试执行能力，无法确定真实修改点。".to_string(),
                    ),
                    blocker_kind: Some("tooling".to_string()),
                    blocker_needs: Some(vec!["仓库读取能力".to_string(), "终端能力".to_string()]),
                    ..SharedTaskUpdatePatch::default()
                },
            )
            .await
            .expect("internal tool availability blocker update should be neutralized");

        assert_eq!(updated.status, TaskStatus::Ready);
        assert_eq!(
            effective_task_closure_state(&updated),
            TaskClosureState::Open
        );
        assert_eq!(updated.task_tool_state.blocker_reason, None);
        assert!(updated.task_tool_state.blocker_needs.is_empty());
        assert_eq!(updated.task_tool_state.blocker_kind, None);
        assert!(updated
            .result_summary
            .as_deref()
            .unwrap_or_default()
            .contains("不能仅因本轮仓库读取或终端工具临时不可用"));
    }

    #[tokio::test]
    async fn update_task_from_tool_rejects_reopening_subtask_after_parent_success() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let child = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", task_draft("child", "done"))
            .await
            .expect("create done child");
        service
            .update_task(
                parent.id.as_str(),
                UpdateTaskRequest {
                    status: Some(TaskStatus::Succeeded),
                    ..UpdateTaskRequest::default()
                },
                None,
            )
            .await
            .expect("complete parent");

        let err = service
            .update_task_from_tool(
                parent.id.as_str(),
                child.id.as_str(),
                SharedTaskUpdatePatch {
                    status: Some("todo".to_string()),
                    ..SharedTaskUpdatePatch::default()
                },
            )
            .await
            .expect_err("subtask should not reopen");

        assert!(err.contains("已经成功"));
        let child_after = service
            .get_task(child.id.as_str())
            .await
            .expect("get child")
            .expect("child");
        assert_eq!(child_after.status, TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn update_task_from_tool_rejects_reopening_succeeded_root_task() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Succeeded).await;

        let err = service
            .update_task_from_tool(
                parent.id.as_str(),
                parent.id.as_str(),
                SharedTaskUpdatePatch {
                    status: Some("doing".to_string()),
                    ..SharedTaskUpdatePatch::default()
                },
            )
            .await
            .expect_err("root task should not reopen");

        assert!(err.contains("已经成功"));
        let parent_after = service
            .get_task(parent.id.as_str())
            .await
            .expect("get parent")
            .expect("parent");
        assert_eq!(parent_after.status, TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn current_run_completion_ignores_open_checklists_from_historical_runs() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let historical = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-old",
                task_draft("historical child", "todo"),
            )
            .await
            .expect("create historical child");

        let completed = service
            .complete_task_from_tool_in_session(
                parent.id.as_str(),
                Some("run-current"),
                parent.id.as_str(),
                None,
            )
            .await
            .expect("historical checklist must not block current run");

        assert_eq!(completed.status, TaskStatus::Succeeded);
        let historical = service
            .get_task(historical.id.as_str())
            .await
            .expect("get historical child")
            .expect("historical child");
        assert_eq!(
            effective_task_closure_state(&historical),
            TaskClosureState::Open
        );
        assert!(task_is_in_session(&historical, "run-old"));
    }

    #[tokio::test]
    async fn terminal_blocker_prevents_parent_success_and_is_reported_by_finalize_tool() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-1",
                task_draft("blocked child", "todo"),
            )
            .await
            .expect("create child");

        service
            .reconcile_tool_tasks(
                parent.id.as_str(),
                "run-1",
                vec![SharedTaskClosureDecision {
                    task_id: child.id.clone(),
                    closure_state: "blocked_terminal".to_string(),
                    reason: "upstream API is unavailable".to_string(),
                    outcome_summary: String::new(),
                    outcome_items: Vec::new(),
                    resume_hint: "retry after upstream recovery".to_string(),
                }],
            )
            .await
            .expect("reconcile blocker");

        let result = service
            .finalize_tool_task_session(parent.id.as_str(), "run-1")
            .await
            .expect("finalize session");
        assert_eq!(
            result.get("can_parent_succeed").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            result.get("parent_should_block").and_then(Value::as_bool),
            Some(true)
        );

        let err = service
            .complete_task_from_tool_in_session(
                parent.id.as_str(),
                Some("run-1"),
                parent.id.as_str(),
                None,
            )
            .await
            .expect_err("terminal blocker must prevent success");
        assert!(err.contains("应进入 blocked"));
    }

    #[tokio::test]
    async fn terminal_blocker_rejects_internal_tool_availability_reasons() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-1",
                task_draft("blocked child", "todo"),
            )
            .await
            .expect("create child");

        let result = service
            .reconcile_tool_tasks(
                parent.id.as_str(),
                "run-1",
                vec![SharedTaskClosureDecision {
                    task_id: child.id.clone(),
                    closure_state: "blocked_terminal".to_string(),
                    reason: "当前运行未暴露仓库读取工具与终端执行工具，无法继续。".to_string(),
                    outcome_summary: "缺少读取/终端能力。".to_string(),
                    outcome_items: vec![SharedTaskOutcomeItem {
                        kind: "blocker".to_string(),
                        text: "没有终端执行能力，无法运行测试。".to_string(),
                        importance: Some("high".to_string()),
                        refs: Vec::new(),
                    }],
                    resume_hint: "后续暴露读取工具后再重试。".to_string(),
                }],
            )
            .await
            .expect("internal tool availability blocker should be soft-rejected");

        assert_eq!(
            result.get("reconciled").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            result.get("rejected_count").and_then(Value::as_u64),
            Some(1)
        );
        assert!(result
            .get("guidance")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("Temporary absence or rate-limiting"));
        let child = service
            .get_task(child.id.as_str())
            .await
            .expect("get child")
            .expect("child");
        assert_eq!(effective_task_closure_state(&child), TaskClosureState::Open);
    }

    #[tokio::test]
    async fn repeated_no_progress_blocks_only_current_run_required_checklists() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let old = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-old",
                task_draft("old child", "todo"),
            )
            .await
            .expect("create old child");
        let current = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-current",
                task_draft("current child", "todo"),
            )
            .await
            .expect("create current child");

        let blocked = block_open_required_task_session_entries(
            &service.store,
            parent.id.as_str(),
            "run-current",
            "completion gate made no progress",
        )
        .await
        .expect("block current session");

        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].id, current.id);
        assert_eq!(
            effective_task_closure_state(&blocked[0]),
            TaskClosureState::BlockedTerminal
        );
        assert_eq!(blocked[0].status, TaskStatus::Blocked);
        let old = service
            .get_task(old.id.as_str())
            .await
            .expect("get old child")
            .expect("old child");
        assert_eq!(effective_task_closure_state(&old), TaskClosureState::Open);
    }

    #[tokio::test]
    async fn terminal_run_finalization_closes_open_and_terminal_blocked_checklists() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let open_child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-failed",
                task_draft("open child", "todo"),
            )
            .await
            .expect("create open child");
        let blocked_child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-failed",
                task_draft("blocked child", "todo"),
            )
            .await
            .expect("create blocked child");
        service
            .reconcile_tool_tasks(
                parent.id.as_str(),
                "run-failed",
                vec![SharedTaskClosureDecision {
                    task_id: blocked_child.id.clone(),
                    closure_state: "blocked_terminal".to_string(),
                    reason: "verified blocker".to_string(),
                    outcome_summary: String::new(),
                    outcome_items: Vec::new(),
                    resume_hint: String::new(),
                }],
            )
            .await
            .expect("mark blocker");

        let summary = finalize_task_session_entries(
            &service.store,
            parent.id.as_str(),
            "run-failed",
            TaskRunStatus::Failed,
        )
        .await
        .expect("finalize failed session");
        assert_eq!(summary.orphaned, 2);

        for task_id in [open_child.id, blocked_child.id] {
            let task = service
                .get_task(task_id.as_str())
                .await
                .expect("get child")
                .expect("child");
            assert_eq!(
                effective_task_closure_state(&task),
                TaskClosureState::Orphaned
            );
            assert_eq!(task.status, TaskStatus::Cancelled);
        }
        let snapshot = load_task_session_snapshot(&service.store, parent.id.as_str(), "run-failed")
            .await
            .expect("load snapshot");
        assert!(snapshot.open_required.is_empty());
        assert!(snapshot.terminal_blocked.is_empty());
    }

    #[tokio::test]
    async fn cancelled_run_cancels_every_nonclosed_current_session_checklist() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let open_child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-cancelled",
                task_draft("open child", "todo"),
            )
            .await
            .expect("create open child");
        let blocked_child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-cancelled",
                task_draft("blocked child", "todo"),
            )
            .await
            .expect("create blocked child");
        service
            .reconcile_tool_tasks(
                parent.id.as_str(),
                "run-cancelled",
                vec![SharedTaskClosureDecision {
                    task_id: blocked_child.id.clone(),
                    closure_state: "blocked_terminal".to_string(),
                    reason: "verified blocker".to_string(),
                    outcome_summary: String::new(),
                    outcome_items: Vec::new(),
                    resume_hint: String::new(),
                }],
            )
            .await
            .expect("mark blocker");

        let summary = finalize_task_session_entries(
            &service.store,
            parent.id.as_str(),
            "run-cancelled",
            TaskRunStatus::Cancelled,
        )
        .await
        .expect("finalize cancelled session");
        assert_eq!(summary.cancelled, 2);

        for task_id in [open_child.id, blocked_child.id] {
            let task = service
                .get_task(task_id.as_str())
                .await
                .expect("get child")
                .expect("child");
            assert_eq!(
                effective_task_closure_state(&task),
                TaskClosureState::Cancelled
            );
            assert_eq!(task.status, TaskStatus::Cancelled);
        }
    }

    #[tokio::test]
    async fn generic_archive_and_cancel_operations_keep_manager_closure_consistent() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let archived_child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-1",
                task_draft("archive child", "todo"),
            )
            .await
            .expect("create archive child");
        let cancelled_child = service
            .create_followup_task_for_tool(
                parent.id.as_str(),
                "run-1",
                task_draft("cancel child", "todo"),
            )
            .await
            .expect("create cancel child");

        let archived_child = service
            .update_task(
                archived_child.id.as_str(),
                UpdateTaskRequest {
                    status: Some(TaskStatus::Archived),
                    ..UpdateTaskRequest::default()
                },
                None,
            )
            .await
            .expect("archive child")
            .expect("archived child");
        assert_eq!(
            effective_task_closure_state(&archived_child),
            TaskClosureState::Superseded
        );

        let cancelled = service
            .cancel_task(
                cancelled_child.id.as_str(),
                CancelTaskRequest {
                    reason: "no longer needed".to_string(),
                    replacement_task_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("cancel child")
            .expect("cancel response");
        assert_eq!(
            effective_task_closure_state(&cancelled.task),
            TaskClosureState::Cancelled
        );

        let snapshot = load_task_session_snapshot(&service.store, parent.id.as_str(), "run-1")
            .await
            .expect("load snapshot");
        assert!(snapshot.open_required.is_empty());
    }

    #[tokio::test]
    async fn retry_keeps_failed_checklist_isolated_in_previous_session() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let draft = task_draft("implement endpoint", "todo");
        let original = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", draft.clone())
            .await
            .expect("create original child");
        finalize_task_session_entries(
            &service.store,
            parent.id.as_str(),
            "run-1",
            TaskRunStatus::Failed,
        )
        .await
        .expect("finalize previous run");

        let previous_snapshot =
            load_task_session_snapshot(&service.store, parent.id.as_str(), "run-1")
                .await
                .expect("load previous snapshot");
        assert_eq!(previous_snapshot.entries.len(), 1);
        assert_eq!(
            effective_task_closure_state(&previous_snapshot.entries[0]),
            TaskClosureState::Orphaned
        );

        let retried = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-2", draft)
            .await
            .expect("create clean retry checklist");
        assert_ne!(retried.id, original.id);
        let snapshot = load_task_session_snapshot(&service.store, parent.id.as_str(), "run-2")
            .await
            .expect("load retry snapshot");
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].id, retried.id);
    }

    #[tokio::test]
    async fn durable_followup_never_blocks_parent_and_detaches_at_run_end() {
        let service = test_service().await;
        let parent = create_task(&service, "parent", TaskStatus::Ready).await;
        let mut draft = task_draft("future cleanup", "todo");
        draft.scope = "durable_followup".to_string();
        draft.required_for_parent_completion = true;
        let durable = service
            .create_followup_task_for_tool(parent.id.as_str(), "run-1", draft)
            .await
            .expect("create durable followup");

        assert!(!effective_task_required_for_parent_completion(&durable));
        service
            .complete_task_from_tool_in_session(
                parent.id.as_str(),
                Some("run-1"),
                parent.id.as_str(),
                None,
            )
            .await
            .expect("durable followup must not block parent");

        let summary = finalize_task_session_entries(
            &service.store,
            parent.id.as_str(),
            "run-1",
            TaskRunStatus::Succeeded,
        )
        .await
        .expect("detach durable followup");
        assert_eq!(summary.durable_detached, 1);
        let durable = service
            .get_task(durable.id.as_str())
            .await
            .expect("get durable followup")
            .expect("durable followup");
        assert_eq!(durable.task_tool_state.task_session_id, None);
        assert!(!effective_task_required_for_parent_completion(&durable));
        assert_eq!(
            effective_task_closure_state(&durable),
            TaskClosureState::Open
        );
    }
}

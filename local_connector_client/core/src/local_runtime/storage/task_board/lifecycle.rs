// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{Context, Result};
use chatos_mcp::{TaskClosureDecision, TaskDraft, TaskUpdatePatch};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::task_board::{
    normalize_task_draft, normalize_task_patch, validate_terminal_state, LocalTaskBoardTaskRecord,
    LocalTaskBoardTaskRow,
};

use super::super::LocalDatabase;
use super::mutations::{merge_task, persist_task};
use super::validation::{require_local_task_scope, validate_prerequisites};

const MAX_RUN_CHECKLIST_TASKS: i64 = 32;

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct LocalTaskManagerSessionSnapshot {
    pub(crate) entries: Vec<LocalTaskBoardTaskRecord>,
    pub(crate) open_required: Vec<LocalTaskBoardTaskRecord>,
    pub(crate) open_optional: Vec<LocalTaskBoardTaskRecord>,
    pub(crate) terminal_blocked: Vec<LocalTaskBoardTaskRecord>,
}

impl LocalTaskManagerSessionSnapshot {
    pub(crate) fn as_value(&self) -> Value {
        json!({
            "task_count": self.entries.len(),
            "open_required_count": self.open_required.len(),
            "open_optional_count": self.open_optional.len(),
            "terminal_blocked_count": self.terminal_blocked.len(),
            "tasks": self.entries,
            "open_required_tasks": self.open_required,
            "open_optional_tasks": self.open_optional,
            "terminal_blocked_tasks": self.terminal_blocked,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct LocalTaskManagerFinalizeSummary {
    pub(crate) satisfied: usize,
    pub(crate) waived: usize,
    pub(crate) blocked_terminal: usize,
    pub(crate) cancelled: usize,
    pub(crate) orphaned: usize,
    pub(crate) durable_detached: usize,
}

impl LocalTaskManagerFinalizeSummary {
    pub(crate) fn total_changed(&self) -> usize {
        self.satisfied + self.waived + self.cancelled + self.orphaned + self.durable_detached
    }
}

impl LocalDatabase {
    pub(crate) async fn create_local_task_manager_session_tasks(
        &self,
        owner_user_id: &str,
        session_id: &str,
        source_turn_id: &str,
        task_session_id: &str,
        drafts: Vec<TaskDraft>,
    ) -> Result<Vec<LocalTaskBoardTaskRecord>> {
        require_local_task_scope(self, owner_user_id, session_id, Some(source_turn_id)).await?;
        let task_session_id = task_session_id.trim();
        if task_session_id.is_empty() {
            return Err(anyhow::anyhow!("local Task Manager session id is required"));
        }
        let drafts = drafts
            .into_iter()
            .map(normalize_task_draft)
            .collect::<Result<Vec<_>, _>>()
            .map_err(anyhow::Error::msg)?;
        let now = local_now_rfc3339();
        let mut ids = Vec::with_capacity(drafts.len());
        let mut transaction = self
            .begin_write()
            .await
            .context("begin local Task Manager session task creation")?;
        let mut checklist_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM task_board_tasks
            WHERE owner_user_id = ? AND session_id = ? AND task_session_id = ?
              AND manager_scope = 'run_checklist'
            "#,
        )
        .bind(owner_user_id)
        .bind(session_id)
        .bind(task_session_id)
        .fetch_one(&mut *transaction)
        .await
        .context("count local Task Manager run checklist")?;

        for draft in drafts {
            let manager_scope = normalize_manager_scope(draft.scope.as_str())?;
            let required_for_parent_completion =
                manager_scope == "run_checklist" && draft.required_for_parent_completion;
            let idempotency_key = normalized_optional(draft.idempotency_key.clone())
                .unwrap_or_else(|| task_semantic_fingerprint(&draft.title, &draft.details));
            let existing_id = sqlx::query_scalar::<_, String>(
                r#"
                SELECT id FROM task_board_tasks
                WHERE owner_user_id = ? AND session_id = ? AND task_session_id = ?
                  AND idempotency_key = ?
                LIMIT 1
                "#,
            )
            .bind(owner_user_id)
            .bind(session_id)
            .bind(task_session_id)
            .bind(idempotency_key.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .context("find idempotent local Task Manager task")?;
            if let Some(existing_id) = existing_id {
                ids.push(existing_id);
                continue;
            }
            if manager_scope == "run_checklist" && checklist_count >= MAX_RUN_CHECKLIST_TASKS {
                return Err(anyhow::anyhow!(
                    "当前本地运行最多允许创建 {MAX_RUN_CHECKLIST_TASKS} 个运行内清单任务，请复用或收口已有任务"
                ));
            }
            let task_id = format!("lc_task_{}", Uuid::new_v4());
            validate_prerequisites(
                self,
                owner_user_id,
                session_id,
                task_id.as_str(),
                draft.prerequisite_task_ids.as_slice(),
            )
            .await?;
            validate_session_prerequisites(
                &mut transaction,
                owner_user_id,
                session_id,
                task_session_id,
                draft.prerequisite_task_ids.as_slice(),
            )
            .await?;
            let closure_state = if draft.status == "done" {
                "satisfied"
            } else {
                "open"
            };
            sqlx::query(
                r#"
                INSERT INTO task_board_tasks (
                    id, session_id, turn_id, owner_user_id, title, details,
                    priority, status, tags_json, prerequisite_task_ids_json,
                    due_at, outcome_summary, outcome_items_json, resume_hint,
                    blocker_reason, blocker_needs_json, blocker_kind,
                    completed_at, last_outcome_at, created_at, updated_at,
                    task_kind, manager_scope, task_session_id,
                    required_for_parent_completion, closure_state, closure_reason,
                    idempotency_key, lifecycle_updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                          'task_manager', ?, ?, ?, ?, NULL, ?, ?)
                "#,
            )
            .bind(task_id.as_str())
            .bind(session_id)
            .bind(source_turn_id)
            .bind(owner_user_id)
            .bind(draft.title)
            .bind(draft.details)
            .bind(draft.priority)
            .bind(draft.status)
            .bind(serde_json::to_string(&draft.tags)?)
            .bind(serde_json::to_string(&draft.prerequisite_task_ids)?)
            .bind(draft.due_at)
            .bind(draft.outcome_summary)
            .bind(serde_json::to_string(&draft.outcome_items)?)
            .bind(draft.resume_hint)
            .bind(draft.blocker_reason)
            .bind(serde_json::to_string(&draft.blocker_needs)?)
            .bind(draft.blocker_kind)
            .bind((closure_state == "satisfied").then(|| now.clone()))
            .bind((closure_state == "satisfied").then(|| now.clone()))
            .bind(now.as_str())
            .bind(now.as_str())
            .bind(manager_scope)
            .bind(task_session_id)
            .bind(required_for_parent_completion)
            .bind(closure_state)
            .bind(idempotency_key)
            .bind(now.as_str())
            .execute(&mut *transaction)
            .await
            .context("create local Task Manager session task")?;
            if manager_scope == "run_checklist" {
                checklist_count += 1;
            }
            ids.push(task_id);
        }
        transaction
            .commit()
            .await
            .context("commit local Task Manager session task creation")?;

        let all = self
            .list_local_task_manager_session_tasks(
                owner_user_id,
                session_id,
                task_session_id,
                true,
                200,
            )
            .await?;
        let by_id = all
            .into_iter()
            .map(|task| (task.id.clone(), task))
            .collect::<std::collections::HashMap<_, _>>();
        ids.into_iter()
            .map(|id| {
                by_id
                    .get(id.as_str())
                    .cloned()
                    .context("local Task Manager task was not persisted")
            })
            .collect()
    }

    pub(crate) async fn list_local_task_manager_session_tasks(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<LocalTaskBoardTaskRecord>> {
        require_local_task_scope(self, owner_user_id, session_id, None).await?;
        let rows = sqlx::query_as::<_, LocalTaskBoardTaskRow>(
            r#"
            SELECT tasks.id, tasks.session_id, tasks.turn_id, turns.user_message_id AS source_user_message_id,
                   tasks.title, tasks.details, tasks.priority, tasks.status,
                   tasks.tags_json, tasks.prerequisite_task_ids_json, tasks.due_at,
                   tasks.outcome_summary, tasks.outcome_items_json, tasks.resume_hint,
                   tasks.blocker_reason, tasks.blocker_needs_json, tasks.blocker_kind,
                   tasks.completed_at, tasks.last_outcome_at, tasks.created_at, tasks.updated_at,
                   tasks.task_kind, tasks.objective, tasks.model_config_id,
                   tasks.is_planning_task, tasks.enabled_builtin_kinds_json,
                   tasks.external_mcp_config_ids_json, tasks.selected_skill_ids_json,
                   tasks.last_run_id, tasks.project_work_item_id, tasks.requirement_id,
                   tasks.execution_group_id, tasks.execution_client_ref,
                   tasks.dependency_context_refs_json, tasks.manager_scope,
                   tasks.task_session_id, tasks.required_for_parent_completion,
                   tasks.closure_state, tasks.closure_reason, tasks.idempotency_key,
                   tasks.lifecycle_updated_at
            FROM task_board_tasks AS tasks
            INNER JOIN turns ON turns.id = tasks.turn_id
            WHERE tasks.owner_user_id = ? AND tasks.session_id = ?
              AND tasks.task_session_id = ? AND tasks.task_kind = 'task_manager'
              AND (? = 1 OR COALESCE(tasks.closure_state, 'open') IN ('open', 'blocked_terminal'))
            ORDER BY tasks.created_at ASC, tasks.id ASC
            LIMIT ?
            "#,
        )
        .bind(owner_user_id)
        .bind(session_id)
        .bind(task_session_id)
        .bind(include_done)
        .bind(limit.clamp(1, 200) as i64)
        .fetch_all(self.pool())
        .await
        .context("list local Task Manager session tasks")?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn update_local_task_manager_session_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
        task_id: &str,
        patch: TaskUpdatePatch,
    ) -> Result<LocalTaskBoardTaskRecord> {
        let current = self
            .select_local_task_manager_session_task(
                owner_user_id,
                session_id,
                task_session_id,
                task_id,
            )
            .await?
            .context("local Task Manager task was not found in the current run session")?;
        let previous_status = current.status.clone();
        let patch = normalize_task_patch(patch).map_err(anyhow::Error::msg)?;
        let mut next = merge_task(current, patch);
        validate_terminal_state(
            next.status.as_str(),
            next.outcome_summary.as_str(),
            next.outcome_items.as_slice(),
            next.blocker_reason.as_str(),
        )
        .map_err(anyhow::Error::msg)?;
        if next.status == "done" {
            next.closure_state = Some("satisfied".to_string());
            next.closure_reason = None;
            next.lifecycle_updated_at = Some(next.updated_at.clone());
        } else if next.status != previous_status {
            next.closure_state = Some("open".to_string());
            next.closure_reason = None;
            next.completed_at = None;
            next.lifecycle_updated_at = Some(next.updated_at.clone());
        }
        persist_task(self, owner_user_id, next, Some(task_session_id)).await
    }

    pub(crate) async fn complete_local_task_manager_session_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
        task_id: &str,
        mut patch: TaskUpdatePatch,
    ) -> Result<LocalTaskBoardTaskRecord> {
        patch.status = Some("done".to_string());
        self.update_local_task_manager_session_task(
            owner_user_id,
            session_id,
            task_session_id,
            task_id,
            patch,
        )
        .await
    }

    pub(crate) async fn delete_local_task_manager_session_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
        task_id: &str,
    ) -> Result<bool> {
        require_local_task_scope(self, owner_user_id, session_id, None).await?;
        let result = sqlx::query(
            r#"
            DELETE FROM task_board_tasks
            WHERE id = ? AND owner_user_id = ? AND session_id = ?
              AND task_session_id = ? AND task_kind = 'task_manager'
            "#,
        )
        .bind(task_id)
        .bind(owner_user_id)
        .bind(session_id)
        .bind(task_session_id)
        .execute(self.pool())
        .await
        .context("delete local Task Manager session task")?;
        Ok(result.rows_affected() > 0)
    }

    pub(crate) async fn reconcile_local_task_manager_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
        decisions: Vec<TaskClosureDecision>,
    ) -> Result<Value> {
        require_local_task_scope(self, owner_user_id, session_id, None).await?;
        let mut seen = HashSet::new();
        let mut transaction = self
            .begin_write()
            .await
            .context("begin local Task Manager reconciliation")?;
        let mut prepared = Vec::with_capacity(decisions.len());
        for decision in decisions {
            let task_id = decision.task_id.trim();
            if task_id.is_empty() {
                return Err(anyhow::anyhow!("task_id is required"));
            }
            if !seen.insert(task_id.to_string()) {
                return Err(anyhow::anyhow!(
                    "同一个任务不能在一次收口请求中重复出现: {task_id}"
                ));
            }
            let exists = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*) FROM task_board_tasks
                WHERE id = ? AND owner_user_id = ? AND session_id = ?
                  AND task_session_id = ? AND task_kind = 'task_manager'
                "#,
            )
            .bind(task_id)
            .bind(owner_user_id)
            .bind(session_id)
            .bind(task_session_id)
            .fetch_one(&mut *transaction)
            .await
            .context("validate local Task Manager reconciliation task")?;
            if exists == 0 {
                return Err(anyhow::anyhow!("task_not_found"));
            }
            let closure_state = normalize_closure_state(decision.closure_state.as_str())?;
            if closure_state != "satisfied" && decision.reason.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "任务 {task_id} 以 {closure_state} 收口时必须提供 reason"
                ));
            }
            prepared.push((task_id.to_string(), decision, closure_state));
        }

        let now = local_now_rfc3339();
        for (task_id, decision, closure_state) in &prepared {
            let status = status_for_closure(closure_state);
            let reason = normalized_optional(Some(decision.reason.clone()));
            let outcome_summary = normalized_optional(Some(decision.outcome_summary.clone()));
            let outcome_items = (!decision.outcome_items.is_empty())
                .then(|| serde_json::to_string(&decision.outcome_items))
                .transpose()?;
            let resume_hint = normalized_optional(Some(decision.resume_hint.clone()));
            sqlx::query(
                r#"
                UPDATE task_board_tasks SET status = ?, closure_state = ?, closure_reason = ?,
                    outcome_summary = COALESCE(?, outcome_summary),
                    outcome_items_json = COALESCE(?, outcome_items_json),
                    resume_hint = COALESCE(?, resume_hint),
                    blocker_reason = CASE WHEN ? = 'blocked_terminal' THEN ?
                                          WHEN ? = 'satisfied' THEN '' ELSE blocker_reason END,
                    completed_at = ?,
                    last_outcome_at = CASE WHEN ? IS NOT NULL OR ? IS NOT NULL THEN ?
                                           ELSE last_outcome_at END,
                    lifecycle_updated_at = ?, updated_at = ?
                WHERE id = ? AND owner_user_id = ? AND session_id = ?
                  AND task_session_id = ? AND task_kind = 'task_manager'
                "#,
            )
            .bind(status)
            .bind(closure_state)
            .bind(reason.as_deref())
            .bind(outcome_summary.as_deref())
            .bind(outcome_items.as_deref())
            .bind(resume_hint.as_deref())
            .bind(closure_state)
            .bind(reason.as_deref())
            .bind(closure_state)
            .bind(now.as_str())
            .bind(outcome_summary.as_deref())
            .bind(outcome_items.as_deref())
            .bind(now.as_str())
            .bind(now.as_str())
            .bind(now.as_str())
            .bind(task_id)
            .bind(owner_user_id)
            .bind(session_id)
            .bind(task_session_id)
            .execute(&mut *transaction)
            .await
            .context("reconcile local Task Manager task")?;
        }
        transaction
            .commit()
            .await
            .context("commit local Task Manager reconciliation")?;
        let reconciled_ids = prepared
            .iter()
            .map(|(task_id, _, _)| task_id.as_str())
            .collect::<HashSet<_>>();
        let snapshot = self
            .local_task_manager_session_snapshot(owner_user_id, session_id, task_session_id)
            .await?;
        let tasks = snapshot
            .entries
            .iter()
            .filter(|task| reconciled_ids.contains(task.id.as_str()))
            .collect::<Vec<_>>();
        Ok(json!({
            "reconciled": true,
            "reconciled_count": tasks.len(),
            "tasks": tasks,
            "session": snapshot.as_value(),
        }))
    }

    pub(crate) async fn local_task_manager_session_snapshot(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
    ) -> Result<LocalTaskManagerSessionSnapshot> {
        let entries = self
            .list_local_task_manager_session_tasks(
                owner_user_id,
                session_id,
                task_session_id,
                true,
                200,
            )
            .await?;
        let open_required = entries
            .iter()
            .filter(|task| {
                effective_closure_state(task) == "open"
                    && task.required_for_parent_completion
                    && task.manager_scope.as_deref() == Some("run_checklist")
            })
            .cloned()
            .collect();
        let open_optional = entries
            .iter()
            .filter(|task| {
                effective_closure_state(task) == "open" && !task.required_for_parent_completion
            })
            .cloned()
            .collect();
        let terminal_blocked = entries
            .iter()
            .filter(|task| {
                effective_closure_state(task) == "blocked_terminal"
                    && task.required_for_parent_completion
            })
            .cloned()
            .collect();
        Ok(LocalTaskManagerSessionSnapshot {
            entries,
            open_required,
            open_optional,
            terminal_blocked,
        })
    }

    pub(crate) async fn finalize_local_task_manager_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
        terminal_status: &str,
    ) -> Result<LocalTaskManagerFinalizeSummary> {
        let mut transaction = self
            .begin_write()
            .await
            .context("begin local Task Manager session finalization")?;
        let rows = sqlx::query_as::<_, (String, String, bool, Option<String>, String)>(
            r#"
            SELECT id, COALESCE(manager_scope, 'run_checklist'),
                   required_for_parent_completion, closure_state, status
            FROM task_board_tasks
            WHERE owner_user_id = ? AND session_id = ? AND task_session_id = ?
              AND task_kind = 'task_manager'
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .bind(owner_user_id)
        .bind(session_id)
        .bind(task_session_id)
        .fetch_all(&mut *transaction)
        .await
        .context("load local Task Manager session for finalization")?;
        let mut summary = LocalTaskManagerFinalizeSummary::default();
        let now = local_now_rfc3339();
        for (task_id, manager_scope, required, stored_closure, status) in rows {
            let mut closure = stored_closure.clone().unwrap_or_else(|| {
                if status == "done" {
                    "satisfied".to_string()
                } else {
                    "open".to_string()
                }
            });
            if closure == "blocked_terminal" && manager_scope == "run_checklist" && required {
                summary.blocked_terminal += 1;
            }
            if manager_scope == "durable_followup" {
                let result = sqlx::query(
                    r#"
                    UPDATE task_board_tasks SET task_session_id = NULL,
                        required_for_parent_completion = 0,
                        closure_state = CASE WHEN status = 'done' THEN 'satisfied'
                                             ELSE closure_state END,
                        lifecycle_updated_at = ?, updated_at = ?
                    WHERE id = ? AND owner_user_id = ? AND session_id = ?
                      AND task_session_id = ?
                    "#,
                )
                .bind(now.as_str())
                .bind(now.as_str())
                .bind(task_id.as_str())
                .bind(owner_user_id)
                .bind(session_id)
                .bind(task_session_id)
                .execute(&mut *transaction)
                .await
                .context("detach durable local Task Manager follow-up")?;
                summary.durable_detached += result.rows_affected() as usize;
                if status == "done" && closure != "satisfied" {
                    summary.satisfied += 1;
                }
                continue;
            }
            if status == "done" {
                closure = "satisfied".to_string();
            }
            let next_closure = match (terminal_status, closure.as_str()) {
                (_, "satisfied" | "cancelled" | "superseded" | "waived" | "orphaned") => {
                    closure.as_str()
                }
                ("completed" | "succeeded", "open") => "waived",
                ("canceled" | "cancelled", "open" | "blocked_terminal") => "cancelled",
                ("failed" | "interrupted", "open" | "blocked_terminal") => "orphaned",
                ("blocked", "open") => "orphaned",
                ("blocked", "blocked_terminal") => "blocked_terminal",
                _ => closure.as_str(),
            };
            if next_closure == closure && stored_closure.as_deref() == Some(next_closure) {
                continue;
            }
            let reason = finalization_reason(next_closure, terminal_status);
            sqlx::query(
                r#"
                UPDATE task_board_tasks SET status = ?, closure_state = ?, closure_reason = ?,
                    completed_at = CASE WHEN ? = 'open' THEN NULL ELSE ? END,
                    blocker_reason = CASE WHEN ? = 'satisfied' THEN '' ELSE blocker_reason END,
                    lifecycle_updated_at = ?, updated_at = ?
                WHERE id = ? AND owner_user_id = ? AND session_id = ?
                  AND task_session_id = ? AND task_kind = 'task_manager'
                "#,
            )
            .bind(status_for_closure(next_closure))
            .bind(next_closure)
            .bind(reason)
            .bind(next_closure)
            .bind(now.as_str())
            .bind(next_closure)
            .bind(now.as_str())
            .bind(now.as_str())
            .bind(task_id)
            .bind(owner_user_id)
            .bind(session_id)
            .bind(task_session_id)
            .execute(&mut *transaction)
            .await
            .context("finalize local Task Manager task")?;
            match next_closure {
                "satisfied" => summary.satisfied += 1,
                "waived" => summary.waived += 1,
                "cancelled" => summary.cancelled += 1,
                "orphaned" => summary.orphaned += 1,
                _ => {}
            }
        }
        transaction
            .commit()
            .await
            .context("commit local Task Manager session finalization")?;
        Ok(summary)
    }

    pub(crate) async fn adopt_local_task_manager_session_for_retry(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
    ) -> Result<u64> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE task_board_tasks SET status = 'todo', closure_state = 'open',
                closure_reason = NULL, completed_at = NULL,
                blocker_reason = '', blocker_needs_json = '[]', blocker_kind = '',
                lifecycle_updated_at = ?, updated_at = ?
            WHERE owner_user_id = ? AND session_id = ? AND task_session_id = ?
              AND task_kind = 'task_manager' AND manager_scope = 'run_checklist'
              AND COALESCE(closure_state, 'open') IN
                  ('open', 'blocked_terminal', 'cancelled', 'orphaned')
            "#,
        )
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(owner_user_id)
        .bind(session_id)
        .bind(task_session_id)
        .execute(self.pool())
        .await
        .context("adopt local Task Manager session for retry")
        .map(|result| result.rows_affected())
    }

    async fn select_local_task_manager_session_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_session_id: &str,
        task_id: &str,
    ) -> Result<Option<LocalTaskBoardTaskRecord>> {
        Ok(self
            .list_local_task_manager_session_tasks(
                owner_user_id,
                session_id,
                task_session_id,
                true,
                200,
            )
            .await?
            .into_iter()
            .find(|task| task.id == task_id))
    }
}

async fn validate_session_prerequisites(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_user_id: &str,
    session_id: &str,
    task_session_id: &str,
    prerequisite_task_ids: &[String],
) -> Result<()> {
    for prerequisite_task_id in prerequisite_task_ids {
        let valid = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM task_board_tasks
            WHERE id = ? AND owner_user_id = ? AND session_id = ?
              AND task_session_id = ? AND task_kind = 'task_manager'
            "#,
        )
        .bind(prerequisite_task_id)
        .bind(owner_user_id)
        .bind(session_id)
        .bind(task_session_id)
        .fetch_one(&mut **transaction)
        .await
        .context("validate local Task Manager session prerequisite")?;
        if valid == 0 {
            return Err(anyhow::anyhow!(
                "前置任务不属于当前本地运行会话: {prerequisite_task_id}"
            ));
        }
    }
    Ok(())
}

fn normalize_manager_scope(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "run_checklist" => Ok("run_checklist"),
        "durable_followup" => Ok("durable_followup"),
        other => Err(anyhow::anyhow!("unsupported task scope: {other}")),
    }
}

fn normalize_closure_state(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "satisfied" => Ok("satisfied"),
        "blocked_terminal" => Ok("blocked_terminal"),
        "cancelled" => Ok("cancelled"),
        "superseded" => Ok("superseded"),
        "waived" => Ok("waived"),
        other => Err(anyhow::anyhow!("unsupported closure_state: {other}")),
    }
}

fn effective_closure_state(task: &LocalTaskBoardTaskRecord) -> &str {
    task.closure_state
        .as_deref()
        .unwrap_or(if task.status == "done" {
            "satisfied"
        } else {
            "open"
        })
}

fn status_for_closure(closure_state: &str) -> &'static str {
    match closure_state {
        "satisfied" => "done",
        "blocked_terminal" => "blocked",
        "cancelled" | "orphaned" => "cancelled",
        "superseded" | "waived" => "archived",
        _ => "todo",
    }
}

fn finalization_reason(closure_state: &str, terminal_status: &str) -> Option<&'static str> {
    match closure_state {
        "satisfied" => None,
        "waived" => Some("父运行已成功结束；该运行内清单未显式关闭，系统已保留审计记录并自动豁免"),
        "cancelled" => Some("父运行已取消；该运行内清单随运行一起取消"),
        "orphaned" if terminal_status == "blocked" => {
            Some("父运行进入阻塞终态；其余未关闭清单已标记为孤立")
        }
        "orphaned" => Some("父运行失败；该运行内清单已标记为孤立，不再污染后续重试"),
        "blocked_terminal" => Some("该清单包含已确认且当前运行无法解除的终态阻塞"),
        _ => Some("Task Manager 运行会话已完成程序化收口"),
    }
}

fn task_semantic_fingerprint(title: &str, details: &str) -> String {
    fn normalized(value: &str) -> String {
        value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    }
    format!("{}\n{}", normalized(title), normalized(details))
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

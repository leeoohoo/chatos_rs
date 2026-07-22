// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::task_board::{LocalTaskBoardTaskRecord, LocalTaskBoardTaskRow};
use crate::local_runtime::task_runner::{
    CreateLocalConversationTaskInput, EnqueueLocalTaskRunInput,
};

use super::super::LocalDatabase;
use super::queries::select_task;
use super::validation::{require_local_task_scope, validate_prerequisites};

impl LocalDatabase {
    pub(crate) async fn create_local_conversation_task(
        &self,
        input: CreateLocalConversationTaskInput,
    ) -> Result<LocalTaskBoardTaskRecord> {
        require_local_task_scope(
            self,
            input.owner_user_id.as_str(),
            input.session_id.as_str(),
            Some(input.source_turn_id.as_str()),
        )
        .await?;
        let task_id = format!("lc_async_task_{}", Uuid::new_v4());
        validate_prerequisites(
            self,
            input.owner_user_id.as_str(),
            input.session_id.as_str(),
            task_id.as_str(),
            input.prerequisite_task_ids.as_slice(),
        )
        .await?;
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO task_board_tasks (
                id, session_id, turn_id, owner_user_id, title, details,
                priority, status, tags_json, prerequisite_task_ids_json,
                due_at, outcome_summary, outcome_items_json, resume_hint,
                blocker_reason, blocker_needs_json, blocker_kind,
                completed_at, last_outcome_at, created_at, updated_at,
                task_kind, objective, model_config_id, is_planning_task,
                enabled_builtin_kinds_json, external_mcp_config_ids_json,
                selected_skill_ids_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 'todo', ?, ?, NULL, '', '[]', '', '', '[]', '', NULL, NULL, ?, ?,
                      'task_runner', ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(task_id.as_str())
        .bind(input.session_id.as_str())
        .bind(input.source_turn_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.title.as_str())
        .bind(input.description.as_str())
        .bind(priority_name(input.priority))
        .bind(serde_json::to_string(&input.tags)?)
        .bind(serde_json::to_string(&input.prerequisite_task_ids)?)
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(input.objective.as_str())
        .bind(input.model_config_id.as_str())
        .bind(input.is_planning_task)
        .bind(serde_json::to_string(&input.enabled_builtin_kinds)?)
        .bind(serde_json::to_string(&input.external_mcp_config_ids)?)
        .bind(serde_json::to_string(&input.selected_skill_ids)?)
        .execute(self.pool())
        .await
        .context("create local conversation task")?;

        let prompt = conversation_task_prompt(
            input.title.as_str(),
            input.description.as_str(),
            input.objective.as_str(),
        );
        let _run = match self
            .enqueue_local_task_run(EnqueueLocalTaskRunInput {
                owner_user_id: input.owner_user_id.clone(),
                project_id: input.project_id,
                requirement_id: None,
                task_kind: "conversation_task".to_string(),
                task_id: task_id.clone(),
                session_id: input.session_id.clone(),
                execution_group_id: input.source_turn_id,
                priority: input.priority,
                prompt,
                model_config_id: input.model_config_id,
            })
            .await
        {
            Ok(run) => run,
            Err(error) => {
                let _ =
                    sqlx::query("DELETE FROM task_board_tasks WHERE id = ? AND owner_user_id = ?")
                        .bind(task_id.as_str())
                        .bind(input.owner_user_id.as_str())
                        .execute(self.pool())
                        .await;
                return Err(error);
            }
        };
        let task = select_task(
            self,
            input.owner_user_id.as_str(),
            input.session_id.as_str(),
            task_id.as_str(),
        )
        .await?
        .context("local conversation task was not persisted")?;
        Ok(task)
    }

    pub(crate) async fn first_local_conversation_task_for_turn(
        &self,
        owner_user_id: &str,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Option<LocalTaskBoardTaskRecord>> {
        let row = sqlx::query_as::<_, LocalTaskBoardTaskRow>(
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
                   tasks.last_run_id
            FROM task_board_tasks AS tasks
            INNER JOIN turns ON turns.id = tasks.turn_id
            WHERE tasks.owner_user_id = ? AND tasks.session_id = ? AND tasks.turn_id = ?
              AND tasks.task_kind = 'task_runner'
            ORDER BY tasks.created_at ASC, tasks.id ASC
            LIMIT 1
            "#,
        )
        .bind(owner_user_id)
        .bind(session_id)
        .bind(turn_id)
        .fetch_optional(self.pool())
        .await
        .context("find local conversation task for source turn")?;
        Ok(row.map(Into::into))
    }

    pub(crate) async fn list_local_conversation_tasks(
        &self,
        owner_user_id: &str,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<LocalTaskBoardTaskRecord>> {
        Ok(self
            .list_local_task_board_tasks(owner_user_id, session_id, None, true, limit)
            .await?
            .into_iter()
            .filter(|task| task.task_kind == "task_runner")
            .collect())
    }

    pub(crate) async fn set_local_conversation_task_status(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_id: &str,
        status: &str,
        result_summary: Option<&str>,
        error: Option<&str>,
    ) -> Result<Option<LocalTaskBoardTaskRecord>> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE task_board_tasks SET status = ?,
                outcome_summary = COALESCE(?, outcome_summary),
                blocker_reason = COALESCE(?, blocker_reason),
                completed_at = CASE WHEN ? = 'done' THEN ? ELSE NULL END,
                last_outcome_at = CASE WHEN ? IN ('done', 'blocked') THEN ? ELSE last_outcome_at END,
                updated_at = ?
            WHERE id = ? AND owner_user_id = ? AND session_id = ? AND task_kind = 'task_runner'
            "#,
        )
        .bind(status)
        .bind(result_summary)
        .bind(error)
        .bind(status)
        .bind(now.as_str())
        .bind(status)
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(task_id)
        .bind(owner_user_id)
        .bind(session_id)
        .execute(self.pool())
        .await
        .context("update local conversation task status")?;
        select_task(self, owner_user_id, session_id, task_id).await
    }
}

fn priority_name(priority: i64) -> &'static str {
    if priority >= 10 {
        "high"
    } else if priority <= -10 {
        "low"
    } else {
        "medium"
    }
}

fn conversation_task_prompt(title: &str, description: &str, objective: &str) -> String {
    format!(
        "[Local Task Runner]\n任务：{title}\n背景：{description}\n目标：{objective}\n\n请严格使用本任务通过插件管理配置并显式选择的本地工具完成目标，验证真实结果后再给出结论。"
    )
}

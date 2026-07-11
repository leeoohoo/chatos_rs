// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use uuid::Uuid;

use super::super::sqlite_rows::task_runner_link_from_row;
use super::SqliteStore;
use crate::models::*;

impl SqliteStore {
    pub async fn get_task_runner_link_by_task_id(
        &self,
        task_runner_task_id: &str,
    ) -> Result<Option<ProjectWorkItemTaskRunnerLinkRecord>, String> {
        let row = sqlx::query(
            "SELECT * FROM project_work_item_task_runner_links
             WHERE task_runner_task_id = ?1",
        )
        .bind(task_runner_task_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(task_runner_link_from_row))
    }

    pub async fn list_task_runner_links(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<ProjectWorkItemTaskRunnerLinkRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM project_work_item_task_runner_links
             WHERE work_item_id = ?1
             ORDER BY updated_at DESC",
        )
        .bind(work_item_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(task_runner_link_from_row).collect())
    }

    pub async fn upsert_task_runner_link(
        &self,
        work_item_id: &str,
        input: LinkTaskRunnerTaskRequest,
    ) -> Result<ProjectWorkItemTaskRunnerLinkRecord, String> {
        validate_required("task_runner_task_id", &input.task_runner_task_id)?;
        self.get_work_item(work_item_id)
            .await?
            .ok_or_else(|| format!("项目工作项不存在: {work_item_id}"))?;
        let task_runner_task_id = input.task_runner_task_id.trim().to_string();
        let link_type =
            normalized_optional(input.link_type).unwrap_or_else(|| "execution".to_string());
        let now = now_rfc3339();
        let existing = sqlx::query(
            "SELECT * FROM project_work_item_task_runner_links
             WHERE task_runner_task_id = ?1",
        )
        .bind(task_runner_task_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?
        .as_ref()
        .map(task_runner_link_from_row);
        let link = ProjectWorkItemTaskRunnerLinkRecord {
            id: existing
                .as_ref()
                .map(|link| link.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            work_item_id: work_item_id.to_string(),
            task_runner_task_id,
            task_runner_run_id: normalized_optional(input.task_runner_run_id),
            link_type,
            execution_group_id: normalized_optional(input.execution_group_id),
            is_current: input.is_current.unwrap_or(true),
            superseded_at: normalized_optional(input.superseded_at),
            source_session_id: normalized_optional(input.source_session_id),
            source_user_message_id: normalized_optional(input.source_user_message_id),
            task_runner_status: normalized_optional(input.task_runner_status),
            last_callback_event: normalized_optional(input.last_callback_event),
            last_callback_at: normalized_optional(input.last_callback_at),
            last_error_message: normalized_optional(input.last_error_message),
            created_at: existing
                .as_ref()
                .map(|link| link.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO project_work_item_task_runner_links (
                id, work_item_id, task_runner_task_id, task_runner_run_id,
                link_type, execution_group_id, is_current, superseded_at,
                source_session_id, source_user_message_id,
                task_runner_status, last_callback_event, last_callback_at,
                last_error_message, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(task_runner_task_id) DO UPDATE SET
                work_item_id = excluded.work_item_id,
                task_runner_task_id = excluded.task_runner_task_id,
                task_runner_run_id = excluded.task_runner_run_id,
                link_type = excluded.link_type,
                execution_group_id = excluded.execution_group_id,
                is_current = excluded.is_current,
                superseded_at = excluded.superseded_at,
                source_session_id = excluded.source_session_id,
                source_user_message_id = excluded.source_user_message_id,
                task_runner_status = excluded.task_runner_status,
                last_callback_event = excluded.last_callback_event,
                last_callback_at = excluded.last_callback_at,
                last_error_message = excluded.last_error_message,
                updated_at = excluded.updated_at",
        )
        .bind(&link.id)
        .bind(&link.work_item_id)
        .bind(&link.task_runner_task_id)
        .bind(&link.task_runner_run_id)
        .bind(&link.link_type)
        .bind(&link.execution_group_id)
        .bind(if link.is_current { 1_i64 } else { 0_i64 })
        .bind(&link.superseded_at)
        .bind(&link.source_session_id)
        .bind(&link.source_user_message_id)
        .bind(&link.task_runner_status)
        .bind(&link.last_callback_event)
        .bind(&link.last_callback_at)
        .bind(&link.last_error_message)
        .bind(&link.created_at)
        .bind(&link.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(link)
    }

    pub async fn delete_task_runner_link(
        &self,
        work_item_id: &str,
        link_id: &str,
    ) -> Result<bool, String> {
        let result = sqlx::query(
            "DELETE FROM project_work_item_task_runner_links
             WHERE work_item_id = ?1 AND id = ?2",
        )
        .bind(work_item_id)
        .bind(link_id.trim())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::{
    canonical_project_status, LocalWorkItemRecord, UpdateLocalWorkItemInput,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn update_local_work_item(
        &self,
        owner_user_id: &str,
        work_item_id: &str,
        input: UpdateLocalWorkItemInput,
    ) -> Result<Option<LocalWorkItemRecord>> {
        let Some(current) = self
            .get_local_work_item(owner_user_id, work_item_id)
            .await?
        else {
            return Ok(None);
        };
        if let Some(requirement_id) = input.requirement_id.as_deref() {
            let requirement = self
                .get_local_requirement(owner_user_id, requirement_id)
                .await?
                .context("local work item requirement was not found")?;
            if requirement.project_id != current.project_id {
                return Err(anyhow::anyhow!(
                    "local work item cannot move across projects"
                ));
            }
        }
        let now = local_now_rfc3339();
        let tags_json = input.tags.as_ref().map(serde_json::to_string).transpose()?;
        let status = input.status.as_deref().map(canonical_project_status);
        sqlx::query(
            r#"
            UPDATE project_work_items SET
                requirement_id = COALESCE(?, requirement_id),
                title = COALESCE(?, title), description = COALESCE(?, description),
                status = COALESCE(?, status), priority = COALESCE(?, priority),
                assignee_user_id = COALESCE(?, assignee_user_id),
                estimate_points = COALESCE(?, estimate_points), due_at = COALESCE(?, due_at),
                sort_order = COALESCE(?, sort_order), tags_json = COALESCE(?, tags_json),
                is_planning_task = COALESCE(?, is_planning_task),
                archived_at = CASE
                    WHEN ? = 'archived' THEN COALESCE(archived_at, ?)
                    WHEN ? IS NOT NULL THEN NULL ELSE archived_at END,
                updated_at = ?
            WHERE id = ? AND owner_user_id = ?
            "#,
        )
        .bind(input.requirement_id.as_deref())
        .bind(input.title.as_deref())
        .bind(input.description.as_deref())
        .bind(status.as_deref())
        .bind(input.priority)
        .bind(input.assignee_user_id.as_deref())
        .bind(input.estimate_points)
        .bind(input.due_at.as_deref())
        .bind(input.sort_order)
        .bind(tags_json.as_deref())
        .bind(input.is_planning_task)
        .bind(status.as_deref())
        .bind(now.as_str())
        .bind(status.as_deref())
        .bind(now.as_str())
        .bind(work_item_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("update local project work item")?;
        self.get_local_work_item(owner_user_id, work_item_id).await
    }

    pub(crate) async fn archive_local_work_item(
        &self,
        owner_user_id: &str,
        work_item_id: &str,
    ) -> Result<Option<LocalWorkItemRecord>> {
        self.update_local_work_item(
            owner_user_id,
            work_item_id,
            UpdateLocalWorkItemInput {
                status: Some("archived".to_string()),
                ..Default::default()
            },
        )
        .await
    }
}

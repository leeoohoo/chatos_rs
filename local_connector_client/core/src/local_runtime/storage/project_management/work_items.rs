// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::{
    CreateLocalWorkItemInput, LocalWorkItemRecord, LocalWorkItemRow,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn create_local_work_item(
        &self,
        input: CreateLocalWorkItemInput,
    ) -> Result<LocalWorkItemRecord> {
        let requirement = self
            .get_local_requirement(input.owner_user_id.as_str(), input.requirement_id.as_str())
            .await?
            .context("local work item requirement was not found")?;
        let id = format!("lc_work_item_{}", Uuid::new_v4());
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO project_work_items (
                id, project_id, requirement_id, owner_user_id, title,
                description, status, priority, assignee_user_id, estimate_points,
                due_at, sort_order, tags_json, is_planning_task, creator_user_id,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.as_str())
        .bind(requirement.project_id.as_str())
        .bind(input.requirement_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.title.as_str())
        .bind(input.description.as_deref())
        .bind(input.status.as_str())
        .bind(input.priority)
        .bind(input.assignee_user_id.as_deref())
        .bind(input.estimate_points)
        .bind(input.due_at.as_deref())
        .bind(input.sort_order)
        .bind(serde_json::to_string(&input.tags)?)
        .bind(input.is_planning_task)
        .bind(input.owner_user_id.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("create local project work item")?;
        self.get_local_work_item(input.owner_user_id.as_str(), id.as_str())
            .await?
            .context("local project work item was not persisted")
    }

    pub(crate) async fn list_local_work_items_for_requirement(
        &self,
        owner_user_id: &str,
        project_id: &str,
        requirement_id: &str,
        include_archived: bool,
    ) -> Result<Vec<LocalWorkItemRecord>> {
        self.require_local_project(owner_user_id, project_id)
            .await?;
        self.list_local_work_items(
            owner_user_id,
            project_id,
            Some(requirement_id),
            include_archived,
        )
        .await
    }

    pub(crate) async fn list_local_project_work_items(
        &self,
        owner_user_id: &str,
        project_id: &str,
        include_archived: bool,
    ) -> Result<Vec<LocalWorkItemRecord>> {
        self.list_local_work_items(owner_user_id, project_id, None, include_archived)
            .await
    }

    async fn list_local_work_items(
        &self,
        owner_user_id: &str,
        project_id: &str,
        requirement_id: Option<&str>,
        include_archived: bool,
    ) -> Result<Vec<LocalWorkItemRecord>> {
        let requirement_filter = if requirement_id.is_some() {
            "AND requirement_id = ?"
        } else {
            ""
        };
        let archived_filter = if include_archived {
            ""
        } else {
            "AND archived_at IS NULL"
        };
        let sql = work_item_select_sql(requirement_filter, archived_filter);
        let mut query = sqlx::query_as::<_, LocalWorkItemRow>(sql.as_str())
            .bind(owner_user_id)
            .bind(project_id);
        if let Some(requirement_id) = requirement_id {
            query = query.bind(requirement_id);
        }
        Ok(query
            .fetch_all(self.pool())
            .await
            .context("list local project work items")?
            .into_iter()
            .map(LocalWorkItemRecord::from)
            .collect())
    }

    pub(crate) async fn get_local_work_item(
        &self,
        owner_user_id: &str,
        work_item_id: &str,
    ) -> Result<Option<LocalWorkItemRecord>> {
        Ok(sqlx::query_as::<_, LocalWorkItemRow>(
            r#"
            SELECT id, project_id, requirement_id, title, description, status,
                   priority, assignee_user_id, estimate_points, due_at, sort_order,
                   tags_json, is_planning_task, creator_user_id, owner_user_id,
                   created_at, updated_at, archived_at
            FROM project_work_items WHERE id = ? AND owner_user_id = ?
            "#,
        )
        .bind(work_item_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local project work item")?
        .map(LocalWorkItemRecord::from))
    }
}

fn work_item_select_sql(requirement_filter: &str, archived_filter: &str) -> String {
    format!(
        r#"
        SELECT id, project_id, requirement_id, title, description, status,
               priority, assignee_user_id, estimate_points, due_at, sort_order,
               tags_json, is_planning_task, creator_user_id, owner_user_id,
               created_at, updated_at, archived_at
        FROM project_work_items
        WHERE owner_user_id = ? AND project_id = ? {requirement_filter} {archived_filter}
        ORDER BY sort_order ASC, priority DESC, created_at ASC, id ASC
        "#
    )
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::{
    canonical_project_status, CreateLocalRequirementInput, LocalRequirementRecord,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn create_local_requirement(
        &self,
        input: CreateLocalRequirementInput,
    ) -> Result<LocalRequirementRecord> {
        self.require_local_project(input.owner_user_id.as_str(), input.project_id.as_str())
            .await?;
        if let Some(parent_id) = input.parent_requirement_id.as_deref() {
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM project_requirements WHERE id = ? AND project_id = ? AND owner_user_id = ?",
            )
            .bind(parent_id)
            .bind(input.project_id.as_str())
            .bind(input.owner_user_id.as_str())
            .fetch_one(self.pool())
            .await
            .context("validate local parent requirement")?;
            if count == 0 {
                return Err(anyhow::anyhow!("local parent requirement was not found"));
            }
        }
        let id = format!("lc_requirement_{}", Uuid::new_v4());
        let now = local_now_rfc3339();
        let status = canonical_project_status(input.status.as_str());
        sqlx::query(
            r#"
            INSERT INTO project_requirements (
                id, project_id, owner_user_id, parent_requirement_id,
                requirement_type, title, summary, detail, business_value,
                acceptance_criteria, source, priority, status, creator_user_id,
                assignee_user_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.as_str())
        .bind(input.project_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.parent_requirement_id.as_deref())
        .bind(input.requirement_type.as_str())
        .bind(input.title.as_str())
        .bind(input.summary.as_deref())
        .bind(input.detail.as_deref())
        .bind(input.business_value.as_deref())
        .bind(input.acceptance_criteria.as_deref())
        .bind(input.source.as_deref())
        .bind(input.priority)
        .bind(status.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.assignee_user_id.as_deref())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("create local project requirement")?;
        self.get_local_requirement(input.owner_user_id.as_str(), id.as_str())
            .await?
            .context("local project requirement was not persisted")
    }

    pub(crate) async fn list_local_requirements(
        &self,
        owner_user_id: &str,
        project_id: &str,
        include_archived: bool,
    ) -> Result<Vec<LocalRequirementRecord>> {
        self.require_local_project(owner_user_id, project_id)
            .await?;
        let archived_filter = if include_archived {
            ""
        } else {
            "AND archived_at IS NULL"
        };
        let sql = format!(
            r#"
            SELECT id, project_id, parent_requirement_id, requirement_type,
                   title, summary, detail, business_value, acceptance_criteria,
                   source, priority, status, creator_user_id,
                   owner_user_id, assignee_user_id, created_at, updated_at, archived_at
            FROM project_requirements
            WHERE owner_user_id = ? AND project_id = ? {archived_filter}
            ORDER BY priority DESC, created_at ASC, id ASC
            "#
        );
        sqlx::query_as::<_, LocalRequirementRecord>(sql.as_str())
            .bind(owner_user_id)
            .bind(project_id)
            .fetch_all(self.pool())
            .await
            .context("list local project requirements")
    }

    pub(crate) async fn get_local_requirement(
        &self,
        owner_user_id: &str,
        requirement_id: &str,
    ) -> Result<Option<LocalRequirementRecord>> {
        sqlx::query_as::<_, LocalRequirementRecord>(
            r#"
            SELECT id, project_id, parent_requirement_id, requirement_type,
                   title, summary, detail, business_value, acceptance_criteria,
                   source, priority, status, creator_user_id,
                   owner_user_id, assignee_user_id, created_at, updated_at, archived_at
            FROM project_requirements
            WHERE id = ? AND owner_user_id = ?
            "#,
        )
        .bind(requirement_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local project requirement")
    }

    pub(super) async fn require_local_project(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<()> {
        if self.get_project(project_id, owner_user_id).await?.is_none() {
            return Err(anyhow::anyhow!("local project was not found"));
        }
        Ok(())
    }
}

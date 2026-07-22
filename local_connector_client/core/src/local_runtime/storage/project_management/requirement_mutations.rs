// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::{
    canonical_project_status, LocalRequirementRecord, UpdateLocalRequirementInput,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn update_local_requirement(
        &self,
        owner_user_id: &str,
        requirement_id: &str,
        input: UpdateLocalRequirementInput,
    ) -> Result<Option<LocalRequirementRecord>> {
        let Some(current) = self
            .get_local_requirement(owner_user_id, requirement_id)
            .await?
        else {
            return Ok(None);
        };
        if let Some(parent_id) = input.parent_requirement_id.as_deref() {
            if parent_id == requirement_id {
                return Err(anyhow::anyhow!("requirement cannot be its own parent"));
            }
            let parent = self
                .get_local_requirement(owner_user_id, parent_id)
                .await?
                .context("local parent requirement was not found")?;
            if parent.project_id != current.project_id {
                return Err(anyhow::anyhow!(
                    "local parent requirement belongs to another project"
                ));
            }
        }
        let now = local_now_rfc3339();
        let status = input.status.as_deref().map(canonical_project_status);
        sqlx::query(
            r#"
            UPDATE project_requirements SET
                parent_requirement_id = COALESCE(?, parent_requirement_id),
                requirement_type = COALESCE(?, requirement_type),
                title = COALESCE(?, title), summary = COALESCE(?, summary),
                detail = COALESCE(?, detail), business_value = COALESCE(?, business_value),
                acceptance_criteria = COALESCE(?, acceptance_criteria),
                source = COALESCE(?, source), priority = COALESCE(?, priority),
                status = COALESCE(?, status), assignee_user_id = COALESCE(?, assignee_user_id),
                archived_at = CASE
                    WHEN ? = 'archived' THEN COALESCE(archived_at, ?)
                    WHEN ? IS NOT NULL THEN NULL ELSE archived_at END,
                updated_at = ?
            WHERE id = ? AND owner_user_id = ?
            "#,
        )
        .bind(input.parent_requirement_id.as_deref())
        .bind(input.requirement_type.as_deref())
        .bind(input.title.as_deref())
        .bind(input.summary.as_deref())
        .bind(input.detail.as_deref())
        .bind(input.business_value.as_deref())
        .bind(input.acceptance_criteria.as_deref())
        .bind(input.source.as_deref())
        .bind(input.priority)
        .bind(status.as_deref())
        .bind(input.assignee_user_id.as_deref())
        .bind(status.as_deref())
        .bind(now.as_str())
        .bind(status.as_deref())
        .bind(now.as_str())
        .bind(requirement_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("update local project requirement")?;
        self.get_local_requirement(owner_user_id, requirement_id)
            .await
    }

    pub(crate) async fn archive_local_requirement(
        &self,
        owner_user_id: &str,
        requirement_id: &str,
    ) -> Result<Option<LocalRequirementRecord>> {
        self.update_local_requirement(
            owner_user_id,
            requirement_id,
            UpdateLocalRequirementInput {
                status: Some("archived".to_string()),
                ..Default::default()
            },
        )
        .await
    }
}

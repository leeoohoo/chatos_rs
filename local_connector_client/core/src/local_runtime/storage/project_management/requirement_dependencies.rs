// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::LocalRequirementDependencyRecord;

use super::super::LocalDatabase;
use super::dependency_validation::{ensure_acyclic, normalized_ids};

impl LocalDatabase {
    pub(crate) async fn set_local_requirement_dependencies(
        &self,
        owner_user_id: &str,
        project_id: &str,
        requirement_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<Vec<LocalRequirementDependencyRecord>> {
        let target = self
            .get_local_requirement(owner_user_id, requirement_id)
            .await?
            .context("local requirement was not found")?;
        if target.project_id != project_id {
            return Err(anyhow::anyhow!(
                "local requirement belongs to another project"
            ));
        }
        let prerequisite_ids = normalized_ids(prerequisite_ids);
        for prerequisite_id in &prerequisite_ids {
            let record = self
                .get_local_requirement(owner_user_id, prerequisite_id)
                .await?
                .context("local prerequisite requirement was not found")?;
            if record.project_id != project_id {
                return Err(anyhow::anyhow!(
                    "local prerequisite requirement belongs to another project"
                ));
            }
        }
        let edges = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT dependencies.prerequisite_requirement_id, dependencies.requirement_id
            FROM requirement_dependencies AS dependencies
            INNER JOIN project_requirements AS requirements
              ON requirements.id = dependencies.requirement_id
            WHERE requirements.owner_user_id = ? AND requirements.project_id = ?
              AND dependencies.requirement_id <> ?
            "#,
        )
        .bind(owner_user_id)
        .bind(project_id)
        .bind(requirement_id)
        .fetch_all(self.pool())
        .await
        .context("load local requirement dependency graph")?;
        ensure_acyclic(
            requirement_id,
            prerequisite_ids.as_slice(),
            edges.as_slice(),
        )?;
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("begin requirement dependencies")?;
        sqlx::query("DELETE FROM requirement_dependencies WHERE requirement_id = ?")
            .bind(requirement_id)
            .execute(&mut *transaction)
            .await
            .context("clear local requirement dependencies")?;
        for prerequisite_id in &prerequisite_ids {
            sqlx::query(
                "INSERT INTO requirement_dependencies (requirement_id, prerequisite_requirement_id, relation_type, created_at) VALUES (?, ?, 'blocks', ?)",
            )
            .bind(requirement_id)
            .bind(prerequisite_id)
            .bind(now.as_str())
            .execute(&mut *transaction)
            .await
            .context("insert local requirement dependency")?;
        }
        transaction
            .commit()
            .await
            .context("commit requirement dependencies")?;
        self.list_local_requirement_dependencies(requirement_id)
            .await
    }

    pub(crate) async fn list_local_requirement_dependencies(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<LocalRequirementDependencyRecord>> {
        sqlx::query_as::<_, LocalRequirementDependencyRecord>(
            r#"
            SELECT requirement_id, prerequisite_requirement_id, relation_type, created_at
            FROM requirement_dependencies WHERE requirement_id = ?
            ORDER BY prerequisite_requirement_id ASC
            "#,
        )
        .bind(requirement_id)
        .fetch_all(self.pool())
        .await
        .context("list local requirement dependencies")
    }
}

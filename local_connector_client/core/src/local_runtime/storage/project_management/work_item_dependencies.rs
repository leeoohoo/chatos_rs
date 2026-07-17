// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::LocalWorkItemDependencyRecord;

use super::super::LocalDatabase;
use super::dependency_validation::{ensure_acyclic, normalized_ids};

impl LocalDatabase {
    pub(crate) async fn set_local_work_item_dependencies(
        &self,
        owner_user_id: &str,
        project_id: &str,
        work_item_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<Vec<LocalWorkItemDependencyRecord>> {
        let target = self
            .get_local_work_item(owner_user_id, work_item_id)
            .await?
            .context("local work item was not found")?;
        if target.project_id != project_id {
            return Err(anyhow::anyhow!(
                "local work item belongs to another project"
            ));
        }
        let prerequisite_ids = normalized_ids(prerequisite_ids);
        for prerequisite_id in &prerequisite_ids {
            let record = self
                .get_local_work_item(owner_user_id, prerequisite_id)
                .await?
                .context("local prerequisite work item was not found")?;
            if record.project_id != project_id {
                return Err(anyhow::anyhow!(
                    "local prerequisite work item belongs to another project"
                ));
            }
        }
        let edges = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT dependencies.prerequisite_work_item_id, dependencies.work_item_id
            FROM work_item_dependencies AS dependencies
            INNER JOIN project_work_items AS items ON items.id = dependencies.work_item_id
            WHERE items.owner_user_id = ? AND items.project_id = ?
              AND dependencies.work_item_id <> ?
            "#,
        )
        .bind(owner_user_id)
        .bind(project_id)
        .bind(work_item_id)
        .fetch_all(self.pool())
        .await
        .context("load local work item dependency graph")?;
        ensure_acyclic(work_item_id, prerequisite_ids.as_slice(), edges.as_slice())?;
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("begin work item dependencies")?;
        sqlx::query("DELETE FROM work_item_dependencies WHERE work_item_id = ?")
            .bind(work_item_id)
            .execute(&mut *transaction)
            .await
            .context("clear local work item dependencies")?;
        for prerequisite_id in &prerequisite_ids {
            sqlx::query(
                "INSERT INTO work_item_dependencies (work_item_id, prerequisite_work_item_id, relation_type, created_at) VALUES (?, ?, 'blocks', ?)",
            )
            .bind(work_item_id)
            .bind(prerequisite_id)
            .bind(now.as_str())
            .execute(&mut *transaction)
            .await
            .context("insert local work item dependency")?;
        }
        transaction
            .commit()
            .await
            .context("commit work item dependencies")?;
        self.list_local_work_item_dependencies(work_item_id).await
    }

    pub(crate) async fn list_local_work_item_dependencies(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<LocalWorkItemDependencyRecord>> {
        sqlx::query_as::<_, LocalWorkItemDependencyRecord>(
            r#"
            SELECT work_item_id, prerequisite_work_item_id, relation_type, created_at
            FROM work_item_dependencies WHERE work_item_id = ?
            ORDER BY prerequisite_work_item_id ASC
            "#,
        )
        .bind(work_item_id)
        .fetch_all(self.pool())
        .await
        .context("list local work item dependencies")
    }
}

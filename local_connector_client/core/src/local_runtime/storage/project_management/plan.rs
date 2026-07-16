// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_runtime::project_management::{
    build_local_dependency_graph, LocalProjectPlanSnapshot,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn local_project_plan(
        &self,
        owner_user_id: &str,
        project_id: &str,
        include_archived: bool,
    ) -> Result<LocalProjectPlanSnapshot> {
        let requirements = self
            .list_local_requirements(owner_user_id, project_id, include_archived)
            .await?;
        let work_items = self
            .list_local_project_work_items(owner_user_id, project_id, include_archived)
            .await?;
        let requirement_dependencies = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT dependencies.requirement_id,
                   dependencies.prerequisite_requirement_id,
                   dependencies.relation_type
            FROM requirement_dependencies AS dependencies
            INNER JOIN project_requirements AS requirements
              ON requirements.id = dependencies.requirement_id
            WHERE requirements.owner_user_id = ? AND requirements.project_id = ?
            "#,
        )
        .bind(owner_user_id)
        .bind(project_id)
        .fetch_all(self.pool())
        .await
        .context("list local requirement dependencies")?;
        let work_item_dependencies = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT dependencies.work_item_id,
                   dependencies.prerequisite_work_item_id,
                   dependencies.relation_type
            FROM work_item_dependencies AS dependencies
            INNER JOIN project_work_items AS items
              ON items.id = dependencies.work_item_id
            WHERE items.owner_user_id = ? AND items.project_id = ?
            "#,
        )
        .bind(owner_user_id)
        .bind(project_id)
        .fetch_all(self.pool())
        .await
        .context("list local work item dependencies")?;
        let dependency_graph = build_local_dependency_graph(
            project_id,
            requirements.as_slice(),
            work_items.as_slice(),
            requirement_dependencies.as_slice(),
            work_item_dependencies.as_slice(),
        );
        Ok(LocalProjectPlanSnapshot {
            project_id: project_id.to_string(),
            requirements,
            work_items,
            dependency_graph,
        })
    }
}

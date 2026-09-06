// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_task_prerequisites(
        &self,
        task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        self.load_collection_items_with_query(
            &self.task_prerequisites,
            doc! { "task_id": task_id },
            Some(mongo_find_options(
                doc! { "created_at": 1, "prerequisite_task_id": 1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_task_prerequisites_for_tasks(
        &self,
        task_ids: &[String],
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        if task_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.load_collection_items_with_query(
            &self.task_prerequisites,
            doc! { "task_id": { "$in": task_ids } },
            Some(mongo_find_options(
                doc! { "task_id": 1, "created_at": 1, "prerequisite_task_id": 1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_task_dependents(
        &self,
        prerequisite_task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        self.load_collection_items_with_query(
            &self.task_prerequisites,
            doc! { "prerequisite_task_id": prerequisite_task_id },
            Some(mongo_find_options(
                doc! { "created_at": 1, "task_id": 1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn set_task_prerequisites(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        self.task_prerequisites
            .delete_many(doc! { "task_id": task_id }, None)
            .await
            .map_err(|err| err.to_string())?;
        let now = now_rfc3339();
        for prerequisite_task_id in prerequisite_task_ids {
            let record = TaskPrerequisiteRecord {
                task_id: task_id.to_string(),
                prerequisite_task_id,
                created_at: now.clone(),
            };
            self.task_prerequisites
                .insert_one(record, None)
                .await
                .map_err(|err| err.to_string())?;
        }
        self.list_task_prerequisites(task_id).await
    }
}

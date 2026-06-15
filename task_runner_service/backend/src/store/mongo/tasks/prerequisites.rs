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

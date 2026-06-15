use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_task_summaries(
        &self,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        self.aggregate_into_items(&self.tasks, Self::task_summary_pipeline(None, None, None))
            .await
    }

    pub(in crate::store) async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        self.aggregate_into_items(
            &self.tasks,
            Self::task_summary_pipeline(
                Some(build_mongo_task_filter(filters)),
                filters.offset,
                filters.limit,
            ),
        )
        .await
    }

    pub(in crate::store) async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.aggregate_into_items(
            &self.tasks,
            Self::task_summary_pipeline(Some(doc! { "id": { "$in": ids.to_vec() } }), None, None),
        )
        .await
    }

    pub(in crate::store) async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        let mut tags = self
            .tasks
            .distinct("tags", None, None)
            .await
            .map_err(|err| err.to_string())?
            .into_iter()
            .filter_map(|value| match value {
                Bson::String(tag) => Some(tag),
                _ => None,
            })
            .collect::<Vec<_>>();
        tags.sort();
        Ok(tags)
    }
}

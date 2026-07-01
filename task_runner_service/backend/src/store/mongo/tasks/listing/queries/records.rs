// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        self.load_collection_items_with_query(
            &self.tasks,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        let filter = build_mongo_task_filter(filters);
        self.load_collection_items_with_query(
            &self.tasks,
            filter,
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                filters.offset,
                filters.limit,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        let filter = build_mongo_task_filter(filters);
        let total = self
            .tasks
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())? as usize;
        let items = self
            .load_collection_items_with_query(
                &self.tasks,
                filter,
                Some(mongo_find_options(
                    doc! { "updated_at": -1, "id": -1 },
                    filters.offset,
                    filters.limit,
                )),
            )
            .await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    pub(in crate::store) async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        self.find_by_id(&self.tasks, id).await
    }
}

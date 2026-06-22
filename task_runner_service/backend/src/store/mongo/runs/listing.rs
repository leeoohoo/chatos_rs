use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_runs(
        &self,
        task_id: Option<&str>,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let filter = task_id.map_or_else(|| doc! {}, |value| doc! { "task_id": value });
        self.load_collection_items_with_query(
            &self.runs,
            filter,
            Some(mongo_find_options(
                doc! { "created_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let filter = build_mongo_run_filter(filters);
        self.load_collection_items_with_query(
            &self.runs,
            filter,
            Some(mongo_find_options(
                doc! { "created_at": -1, "id": -1 },
                filters.offset,
                filters.limit,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let filter = build_mongo_run_filter(filters);
        let total = self
            .runs
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())? as usize;
        let items = self
            .load_collection_items_with_query(
                &self.runs,
                filter,
                Some(mongo_find_options(
                    doc! { "created_at": -1, "id": -1 },
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

    pub(in crate::store) async fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        self.aggregate_into_items(
            &self.runs,
            Self::run_summary_pipeline(
                Some(build_mongo_run_filter(filters)),
                filters.offset,
                filters.limit,
            ),
        )
        .await
    }

    pub(in crate::store) async fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RunSummaryRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.aggregate_into_items(
            &self.runs,
            Self::run_summary_pipeline(Some(doc! { "id": { "$in": ids.to_vec() } }), None, None),
        )
        .await
    }

    pub(in crate::store) async fn get_run(
        &self,
        id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.find_by_id(&self.runs, id).await
    }
}

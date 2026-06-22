use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_ui_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        let filter = build_mongo_prompt_filter(task_id, run_id, status);
        self.load_collection_items_with_query(
            &self.ui_prompts,
            filter,
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_ui_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        let filter = build_mongo_prompt_filter(
            filters.task_id.as_deref(),
            filters.run_id.as_deref(),
            filters.status,
        );
        let total = self
            .ui_prompts
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())? as usize;
        let items = self
            .load_collection_items_with_query(
                &self.ui_prompts,
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

    pub(in crate::store) async fn get_ui_prompt(
        &self,
        id: &str,
    ) -> Result<Option<UiPromptRecord>, String> {
        self.find_by_id(&self.ui_prompts, id).await
    }

    pub(in crate::store) async fn save_ui_prompt(
        &self,
        prompt: UiPromptRecord,
    ) -> Result<UiPromptRecord, String> {
        self.upsert_by_id(&self.ui_prompts, &prompt.id, &prompt)
            .await?;
        Ok(prompt)
    }

    pub(in crate::store) async fn list_ui_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptTaskCountRecord>, String> {
        let mut match_filter = doc! {
            "task_id": {
                "$exists": true,
                "$ne": Bson::Null,
            }
        };
        if let Some(status) = status {
            match_filter.insert("status", ui_prompt_status_to_str(status));
        }
        let rows = self
            .aggregate_documents(
                &self.ui_prompts,
                vec![
                    doc! { "$match": match_filter },
                    doc! {
                        "$group": {
                            "_id": "$task_id",
                            "prompt_count": { "$sum": 1_i32 },
                        }
                    },
                    doc! { "$sort": { "prompt_count": -1, "_id": 1 } },
                ],
            )
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(UiPromptTaskCountRecord {
                    task_id: bson_string_field(&row, "_id")?,
                    count: bson_usize_field(&row, "prompt_count")?,
                })
            })
            .collect())
    }
}

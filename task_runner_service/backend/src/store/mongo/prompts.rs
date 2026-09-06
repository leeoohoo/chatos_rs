// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_ask_user_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<AskUserPromptStatus>,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        let filter = build_mongo_prompt_filter(task_id, run_id, status);
        self.load_collection_items_with_query(
            &self.ask_user_prompts,
            filter,
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_ask_user_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<AskUserPromptRecord>, String> {
        let filter = build_mongo_prompt_filter(
            filters.task_id.as_deref(),
            filters.run_id.as_deref(),
            filters.status,
        );
        let total = self
            .ask_user_prompts
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())? as usize;
        let items = self
            .load_collection_items_with_query(
                &self.ask_user_prompts,
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

    pub(in crate::store) async fn get_ask_user_prompt(
        &self,
        id: &str,
    ) -> Result<Option<AskUserPromptRecord>, String> {
        self.find_by_id(&self.ask_user_prompts, id).await
    }

    pub(in crate::store) async fn save_ask_user_prompt(
        &self,
        prompt: AskUserPromptRecord,
    ) -> Result<AskUserPromptRecord, String> {
        self.upsert_by_id(&self.ask_user_prompts, &prompt.id, &prompt)
            .await?;
        Ok(prompt)
    }

    pub(in crate::store) async fn prune_terminal_ask_user_prompts_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> Result<AskUserPromptPruneResult, String> {
        let candidate_limit = i64::try_from(candidate_limit)
            .map_err(|_| "Ask User cleanup batch size is too large".to_string())?;
        let eligible_documents = self
            .aggregate_documents(
                &self.ask_user_prompts,
                vec![
                    doc! {
                        "$match": {
                            "status": {
                                "$in": ["submitted", "cancelled", "timed_out", "failed"]
                            },
                            "resolution_event_pending": { "$ne": true },
                            "updated_at": { "$lt": cutoff },
                            "run_id": { "$exists": true, "$ne": Bson::Null },
                        }
                    },
                    doc! { "$project": { "id": 1, "run_id": 1, "updated_at": 1 } },
                    doc! {
                        "$lookup": {
                            "from": "task_runs",
                            "localField": "run_id",
                            "foreignField": "id",
                            "as": "run",
                        }
                    },
                    doc! { "$unwind": "$run" },
                    doc! {
                        "$match": {
                            "run.status": {
                                "$in": ["succeeded", "failed", "cancelled", "blocked"]
                            }
                        }
                    },
                    doc! { "$sort": { "updated_at": 1, "id": 1 } },
                    doc! { "$limit": candidate_limit },
                ],
            )
            .await?;
        let eligible_prompt_ids = eligible_documents
            .into_iter()
            .filter_map(|document| document.get_str("id").ok().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        if eligible_prompt_ids.is_empty() {
            return Ok(AskUserPromptPruneResult::default());
        }

        let result = self
            .ask_user_prompts
            .delete_many(
                doc! {
                    "id": { "$in": &eligible_prompt_ids },
                    "status": {
                        "$in": ["submitted", "cancelled", "timed_out", "failed"]
                    },
                    "resolution_event_pending": { "$ne": true },
                    "updated_at": { "$lt": cutoff },
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(AskUserPromptPruneResult {
            eligible_prompts: eligible_prompt_ids.len(),
            deleted_prompts: result.deleted_count,
        })
    }

    pub(in crate::store) async fn list_pending_ask_user_resolution_events(
        &self,
        limit: usize,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        self.load_collection_items_with_query(
            &self.ask_user_prompts,
            doc! {
                "resolution_event_pending": true,
                "status": { "$ne": "pending" },
            },
            Some(
                FindOptions::builder()
                    .sort(doc! { "updated_at": 1, "id": 1 })
                    .limit(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
                    .build(),
            ),
        )
        .await
    }

    pub(in crate::store) async fn acknowledge_ask_user_resolution_event(
        &self,
        prompt_id: &str,
    ) -> Result<bool, String> {
        self.ask_user_prompts
            .update_one(
                doc! { "id": prompt_id, "resolution_event_pending": true },
                doc! { "$set": { "resolution_event_pending": false } },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn list_ask_user_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
        task_ids: Option<&[String]>,
    ) -> Result<Vec<AskUserPromptTaskCountRecord>, String> {
        if task_ids.is_some_and(|ids| ids.is_empty()) {
            return Ok(Vec::new());
        }
        let mut match_filter = doc! {
            "task_id": {
                "$exists": true,
                "$ne": Bson::Null,
            }
        };
        if let Some(status) = status {
            match_filter.insert("status", ask_user_prompt_status_to_str(status));
        }
        if let Some(task_ids) = task_ids {
            match_filter.insert("task_id", doc! { "$in": task_ids });
        }
        let rows = self
            .aggregate_documents(
                &self.ask_user_prompts,
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
                Some(AskUserPromptTaskCountRecord {
                    task_id: bson_string_field(&row, "_id")?,
                    count: bson_usize_field(&row, "prompt_count")?,
                })
            })
            .collect())
    }
}

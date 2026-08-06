// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn get_run_event(
        &self,
        run_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskRunEventRecord>, String> {
        self.run_events
            .find_one(doc! { "run_id": run_id, "id": event_id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn list_run_events(
        &self,
        run_id: &str,
    ) -> Result<Vec<TaskRunEventRecord>, String> {
        self.load_collection_items_with_query(
            &self.run_events,
            doc! { "run_id": run_id },
            Some(mongo_find_options(
                doc! { "created_at": 1, "id": 1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn list_run_events_after(
        &self,
        run_id: &str,
        after_created_at: Option<&str>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TaskRunEventRecord>, String> {
        let filter = match (after_created_at, after_id) {
            (Some(created_at), Some(id)) => doc! {
                "run_id": run_id,
                "$or": [
                    { "created_at": { "$gt": created_at } },
                    { "created_at": created_at, "id": { "$gt": id } },
                ],
            },
            _ => doc! { "run_id": run_id },
        };
        self.load_collection_items_with_query(
            &self.run_events,
            filter,
            Some(mongo_find_options(
                doc! { "created_at": 1, "id": 1 },
                None,
                Some(limit),
            )),
        )
        .await
    }

    pub(in crate::store) async fn latest_run_event_cursor(
        &self,
        run_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        let mut events = self
            .load_collection_items_with_query(
                &self.run_events,
                doc! { "run_id": run_id },
                Some(mongo_find_options(
                    doc! { "created_at": -1, "id": -1 },
                    None,
                    Some(1),
                )),
            )
            .await?;
        Ok(events.pop().map(|event| (event.created_at, event.id)))
    }

    pub(in crate::store) async fn prune_terminal_run_events_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> Result<RunEventPruneResult, String> {
        let candidate_limit = i64::try_from(candidate_limit)
            .map_err(|_| "run event cleanup batch size is too large".to_string())?;
        let eligible_documents = self
            .aggregate_documents(
                &self.run_events,
                vec![
                    doc! { "$match": { "created_at": { "$lt": cutoff } } },
                    doc! { "$group": { "_id": "$run_id" } },
                    doc! {
                        "$lookup": {
                            "from": "task_runs",
                            "localField": "_id",
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
                    doc! { "$sort": { "_id": 1 } },
                    doc! { "$limit": candidate_limit },
                ],
            )
            .await?;
        let eligible_run_ids = eligible_documents
            .into_iter()
            .filter_map(|document| document.get_str("_id").ok().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        if eligible_run_ids.is_empty() {
            return Ok(RunEventPruneResult::default());
        }

        let result = self
            .run_events
            .delete_many(
                doc! {
                    "run_id": { "$in": &eligible_run_ids },
                    "created_at": { "$lt": cutoff },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(RunEventPruneResult {
            eligible_runs: eligible_run_ids.len(),
            deleted_events: result.deleted_count,
        })
    }

    pub(in crate::store) async fn append_run_event(
        &self,
        event: TaskRunEventRecord,
    ) -> Result<(), String> {
        self.upsert_by_id(&self.run_events, &event.id, &event).await
    }
}

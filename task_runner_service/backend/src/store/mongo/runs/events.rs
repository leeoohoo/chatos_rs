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

    pub(in crate::store) async fn append_run_event(
        &self,
        event: TaskRunEventRecord,
    ) -> Result<(), String> {
        self.upsert_by_id(&self.run_events, &event.id, &event).await
    }
}

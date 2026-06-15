use super::*;

impl MongoStore {
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

    pub(in crate::store) async fn append_run_event(
        &self,
        event: TaskRunEventRecord,
    ) -> Result<(), String> {
        self.upsert_by_id(&self.run_events, &event.id, &event)
            .await?;
        let _ = self.run_event_sender.send(event);
        Ok(())
    }
}

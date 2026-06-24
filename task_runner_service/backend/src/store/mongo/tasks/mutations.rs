use super::*;

impl MongoStore {
    pub(in crate::store) async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        self.upsert_by_id(&self.tasks, &task.id, &task).await?;
        Ok(task)
    }

    pub(in crate::store) async fn delete_task(&self, id: &str) -> Result<bool, String> {
        if self.find_by_id(&self.tasks, id).await?.is_none() {
            return Ok(false);
        }
        let run_ids = self
            .list_runs(Some(id))
            .await?
            .into_iter()
            .map(|run| run.id)
            .collect::<Vec<_>>();

        self.task_prerequisites
            .delete_many(
                doc! {
                    "$or": [
                        doc! { "task_id": id },
                        doc! { "prerequisite_task_id": id },
                    ]
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;

        self.ask_user_prompts
            .delete_many(
                doc! {
                    "$or": [
                        doc! { "task_id": id },
                        doc! { "run_id": { "$in": run_ids.clone() } },
                    ]
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.run_events
            .delete_many(doc! { "run_id": { "$in": run_ids.clone() } }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.runs
            .delete_many(doc! { "task_id": id }, None)
            .await
            .map_err(|err| err.to_string())?;

        {
            let mut cancel_requested_runs = self.cancel_requested_runs.write();
            for run_id in run_ids {
                cancel_requested_runs.remove(&run_id);
            }
        }

        self.delete_by_id(&self.tasks, id).await
    }
}

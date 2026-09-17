// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        let mut document = bson::to_document(&task).map_err(|err| err.to_string())?;
        document.insert("schedule_due_at", schedule_due_at_bson(&task.schedule)?);
        self.tasks
            .clone_with_type::<Document>()
            .replace_one(
                doc! { "id": &task.id },
                document,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(task)
    }

    pub(in crate::store) async fn update_task_schedule_if_next_run_at(
        &self,
        task_id: &str,
        expected_next_run_at: &str,
        schedule: TaskScheduleConfig,
        updated_at: &str,
    ) -> Result<Option<TaskRecord>, String> {
        let result = self
            .tasks
            .update_one(
                doc! {
                    "id": task_id,
                    "schedule.next_run_at": expected_next_run_at,
                },
                doc! {
                    "$set": {
                        "schedule": bson::to_bson(&schedule).map_err(|err| err.to_string())?,
                        "schedule_due_at": schedule_due_at_bson(&schedule)?,
                        "updated_at": updated_at,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.matched_count == 0 {
            return Ok(None);
        }
        self.get_task(task_id).await
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

fn schedule_due_at_bson(schedule: &TaskScheduleConfig) -> Result<Bson, String> {
    let Some(value) = schedule.next_run_at.as_deref() else {
        return Ok(Bson::Null);
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|err| format!("invalid schedule.next_run_at: {err}"))?;
    Ok(Bson::DateTime(mongodb::bson::DateTime::from_millis(
        parsed.timestamp_millis(),
    )))
}

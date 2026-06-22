use super::*;

impl MongoStore {
    pub(in crate::store) async fn save_run(
        &self,
        run: TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        self.runs
            .replace_one(
                doc! { "id": &run.id },
                &run,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| {
                if is_mongo_active_run_conflict(&err.to_string()) {
                    "当前任务已有正在执行的运行".to_string()
                } else {
                    err.to_string()
                }
            })?;
        let mut cancel_requested_runs = self.cancel_requested_runs.write();
        if run.cancel_requested {
            cancel_requested_runs.insert(run.id.clone());
        } else {
            cancel_requested_runs.remove(&run.id);
        }
        Ok(run)
    }
}

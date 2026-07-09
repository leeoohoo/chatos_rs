// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod models;
mod prompts;
mod runs;
mod tasks;
mod users;

impl SqliteStore {
    pub(super) async fn connect(
        database_url: &str,
        run_event_sender: broadcast::Sender<TaskRunEventRecord>,
    ) -> Result<Self, String> {
        ensure_sqlite_parent_dir(database_url)?;
        let connect_options = SqliteConnectOptions::from_str(database_url)
            .map_err(|err| err.to_string())?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(Self {
            pool,
            cancel_requested_runs: Arc::new(RwLock::new(HashSet::new())),
            run_event_sender,
        })
    }

    pub(super) async fn ensure_active_run_index(&self) -> Result<(), String> {
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_task_runs_active_task_unique
             ON task_runs(task_id)
             WHERE status IN ('queued', 'running')",
        )
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
    }
}

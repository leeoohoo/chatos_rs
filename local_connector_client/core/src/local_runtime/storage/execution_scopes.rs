// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn execution_scope_is_terminal(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        generation: i64,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query("DELETE FROM execution_scope_tombstones WHERE expires_at_unix <= ?")
            .bind(now)
            .execute(self.pool())
            .await
            .context("prune expired execution scope tombstones")?;
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM execution_scope_tombstones \
             WHERE owner_user_id = ? AND project_id = ? AND run_id = ? AND generation = ? \
             AND expires_at_unix > ?",
        )
        .bind(owner_user_id)
        .bind(project_id)
        .bind(run_id)
        .bind(generation)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .map(|count| count > 0)
        .context("query execution scope tombstone")
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn persist_execution_scope_tombstone(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        generation: i64,
        terminal_status: &str,
        expires_at_unix: i64,
    ) -> Result<()> {
        let finalized_at_unix = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO execution_scope_tombstones \
             (owner_user_id, project_id, run_id, generation, terminal_status, finalized_at_unix, expires_at_unix) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(owner_user_id, project_id, run_id, generation) DO UPDATE SET \
             terminal_status = excluded.terminal_status, \
             finalized_at_unix = excluded.finalized_at_unix, \
             expires_at_unix = MAX(execution_scope_tombstones.expires_at_unix, excluded.expires_at_unix)",
        )
        .bind(owner_user_id)
        .bind(project_id)
        .bind(run_id)
        .bind(generation)
        .bind(terminal_status)
        .bind(finalized_at_unix)
        .bind(expires_at_unix)
        .execute(self.pool())
        .await
        .context("persist execution scope tombstone")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn terminal_scope_tombstone_is_persistent_and_generation_scoped() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("runtime.sqlite3");
        let database = LocalDatabase::open(path.clone()).await.unwrap();
        database
            .persist_execution_scope_tombstone(
                "user-1",
                "project-1",
                "run-1",
                1,
                "succeeded",
                chrono::Utc::now().timestamp() + 300,
            )
            .await
            .unwrap();
        assert!(database
            .execution_scope_is_terminal("user-1", "project-1", "run-1", 1)
            .await
            .unwrap());
        assert!(!database
            .execution_scope_is_terminal("user-1", "project-1", "run-1", 2)
            .await
            .unwrap());
        database.close().await;

        let reopened = LocalDatabase::open(path).await.unwrap();
        assert!(reopened
            .execution_scope_is_terminal("user-1", "project-1", "run-1", 1)
            .await
            .unwrap());
        reopened.close().await;
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use super::sqlite_util::ensure_sqlite_parent_dir;

const INIT_SQL: &str = include_str!("../../migrations/0001_init.sql");

mod projects;
mod requirements;
mod runtime_environment;
mod work_items;

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn new(database_url: &str) -> Result<Self, String> {
        ensure_sqlite_parent_dir(database_url)?;
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|err| err.to_string())?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|err| err.to_string())?;
        let store = Self { pool };
        store.run_migrations().await?;
        Ok(store)
    }

    async fn run_migrations(&self) -> Result<(), String> {
        for statement in INIT_SQL
            .split(';')
            .map(str::trim)
            .filter(|sql| !sql.is_empty())
        {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(|err| format!("migration failed: {err}; sql={statement}"))?;
        }
        self.ensure_actor_columns().await?;
        self.ensure_project_cloud_columns().await?;
        self.ensure_requirement_documents_multiple_rows().await?;
        self.repair_failed_work_item_statuses().await?;
        self.repair_blocked_requirement_statuses().await?;
        Ok(())
    }

    async fn ensure_actor_columns(&self) -> Result<(), String> {
        for column in [
            "creator_user_id",
            "creator_username",
            "creator_display_name",
        ] {
            self.ensure_text_column("projects", column).await?;
        }
        for table in [
            "project_profiles",
            "requirements",
            "requirement_documents",
            "project_work_items",
        ] {
            for column in [
                "creator_user_id",
                "creator_username",
                "creator_display_name",
                "owner_user_id",
                "owner_username",
                "owner_display_name",
            ] {
                self.ensure_text_column(table, column).await?;
            }
        }
        self.ensure_text_column("requirements", "requirement_type")
            .await?;
        self.ensure_text_column("project_work_items", "task_runner_default_model_config_id")
            .await?;
        self.ensure_text_column("project_work_items", "task_runner_enabled_tool_ids_json")
            .await?;
        self.ensure_text_column("project_work_items", "task_runner_skill_ids_json")
            .await?;
        self.ensure_integer_column_with_default("project_work_items", "is_planning_task", 0)
            .await?;
        for column in [
            "source_session_id",
            "source_user_message_id",
            "task_runner_status",
            "last_callback_event",
            "last_callback_at",
            "last_error_message",
        ] {
            self.ensure_text_column("project_work_item_task_runner_links", column)
                .await?;
        }
        sqlx::query(
            "DELETE FROM project_work_item_task_runner_links
             WHERE rowid NOT IN (
               SELECT MAX(rowid)
               FROM project_work_item_task_runner_links
               GROUP BY work_item_id
             )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_project_work_item_task_runner_links_work_item_unique
             ON project_work_item_task_runner_links(work_item_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_project_work_item_task_runner_links_task_id
             ON project_work_item_task_runner_links(task_runner_task_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn ensure_project_cloud_columns(&self) -> Result<(), String> {
        for column in [
            "source_type",
            "cloud_import_source",
            "import_status",
            "source_git_url",
            "harness_space_identifier",
            "harness_repo_identifier",
            "harness_repo_path",
            "harness_git_url",
            "harness_git_ssh_url",
            "import_error",
            "import_started_at",
            "import_finished_at",
        ] {
            self.ensure_text_column("projects", column).await?;
        }
        Ok(())
    }

    async fn repair_failed_work_item_statuses(&self) -> Result<(), String> {
        sqlx::query(
            "UPDATE project_work_items
             SET status = 'failed',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE status = 'blocked'
               AND id IN (
                 SELECT work_item_id
                 FROM project_work_item_task_runner_links
                 WHERE lower(trim(task_runner_status)) IN ('failed', 'error')
               )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn repair_blocked_requirement_statuses(&self) -> Result<(), String> {
        sqlx::query(
            "WITH RECURSIVE failed_requirements(id) AS (
               SELECT DISTINCT requirement_id
               FROM project_work_items
               WHERE status = 'failed'
               UNION
               SELECT requirements.parent_requirement_id
               FROM requirements
               JOIN failed_requirements ON requirements.id = failed_requirements.id
               WHERE requirements.parent_requirement_id IS NOT NULL
             )
             UPDATE requirements
             SET status = 'failed',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id IN (
                 SELECT id FROM failed_requirements WHERE id IS NOT NULL
             )
               AND status IN ('reviewing', 'approved', 'in_progress', 'blocked')",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;

        sqlx::query(
            "WITH RECURSIVE blocked_requirements(id) AS (
               SELECT DISTINCT requirement_id
               FROM project_work_items
               WHERE status = 'blocked'
               UNION
               SELECT requirements.parent_requirement_id
               FROM requirements
               JOIN blocked_requirements ON requirements.id = blocked_requirements.id
               WHERE requirements.parent_requirement_id IS NOT NULL
             )
             UPDATE requirements
             SET status = 'blocked',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id IN (
                 SELECT id FROM blocked_requirements WHERE id IS NOT NULL
             )
               AND status IN ('reviewing', 'approved', 'in_progress')",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn ensure_requirement_documents_multiple_rows(&self) -> Result<(), String> {
        let indexes = sqlx::query("PRAGMA index_list(requirement_documents)")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let has_legacy_unique = indexes.iter().any(|row| {
            row.get::<i64, _>("unique") == 1 && row.get::<String, _>("origin").as_str() == "u"
        });
        if has_legacy_unique {
            let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
            sqlx::query("ALTER TABLE requirement_documents RENAME TO requirement_documents_legacy")
                .execute(&mut *tx)
                .await
                .map_err(|err| err.to_string())?;
            sqlx::query(
                "CREATE TABLE requirement_documents (
                  id TEXT PRIMARY KEY,
                  requirement_id TEXT NOT NULL,
                  doc_type TEXT NOT NULL DEFAULT 'technical_overview',
                  creator_user_id TEXT,
                  creator_username TEXT,
                  creator_display_name TEXT,
                  owner_user_id TEXT,
                  owner_username TEXT,
                  owner_display_name TEXT,
                  title TEXT NOT NULL,
                  format TEXT NOT NULL DEFAULT 'markdown',
                  content TEXT NOT NULL DEFAULT '',
                  version INTEGER NOT NULL DEFAULT 1,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  FOREIGN KEY(requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
                )",
            )
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
            sqlx::query(
                "INSERT OR IGNORE INTO requirement_documents (
                    id, requirement_id, doc_type,
                    creator_user_id, creator_username, creator_display_name,
                    owner_user_id, owner_username, owner_display_name,
                    title, format, content, version, created_at, updated_at
                 )
                 SELECT
                    id, requirement_id, doc_type,
                    creator_user_id, creator_username, creator_display_name,
                    owner_user_id, owner_username, owner_display_name,
                    title, format, content, version, created_at, updated_at
                 FROM requirement_documents_legacy",
            )
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
            sqlx::query("DROP TABLE requirement_documents_legacy")
                .execute(&mut *tx)
                .await
                .map_err(|err| err.to_string())?;
            tx.commit().await.map_err(|err| err.to_string())?;
        }
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_requirement_documents_requirement_id
             ON requirement_documents(requirement_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_requirement_documents_requirement_type_sort
             ON requirement_documents(requirement_id, doc_type, updated_at DESC, id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn ensure_text_column(&self, table: &str, column: &str) -> Result<(), String> {
        let pragma = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if rows
            .iter()
            .any(|row| row.get::<String, _>("name").as_str() == column)
        {
            return Ok(());
        }
        let statement = format!("ALTER TABLE {table} ADD COLUMN {column} TEXT");
        sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&self.pool)
            .await
            .map_err(|err| format!("migration failed: {err}; sql={statement}"))?;
        Ok(())
    }

    async fn ensure_integer_column_with_default(
        &self,
        table: &str,
        column: &str,
        default_value: i64,
    ) -> Result<(), String> {
        let pragma = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if rows
            .iter()
            .any(|row| row.get::<String, _>("name").as_str() == column)
        {
            return Ok(());
        }
        let statement = format!(
            "ALTER TABLE {table} ADD COLUMN {column} INTEGER NOT NULL DEFAULT {default_value}"
        );
        sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&self.pool)
            .await
            .map_err(|err| format!("migration failed: {err}; sql={statement}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    mod projects;
    mod requirements;
    mod schema;
    mod support;
    mod work_items;
}

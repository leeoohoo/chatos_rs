use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use super::sqlite_util::ensure_sqlite_parent_dir;

const INIT_SQL: &str = include_str!("../../migrations/0001_init.sql");

mod projects;
mod requirements;
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

    async fn ensure_text_column(&self, table: &str, column: &str) -> Result<(), String> {
        let pragma = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(pragma.as_str())
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
        sqlx::query(statement.as_str())
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

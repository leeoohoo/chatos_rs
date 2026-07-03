// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use sqlx::{Row, SqlitePool};

#[path = "sqlite_schema/statements.rs"]
mod statements;

pub(super) async fn create_tables_sqlite(pool: &SqlitePool) -> Result<(), String> {
    for sql in statements::CREATE_TABLES {
        sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| format!("create table failed: {e}"))?;
    }

    ensure_column(
        pool,
        "session_runtime_settings",
        "reasoning_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "session_runtime_settings",
        "plan_mode_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await
    .ok();
    ensure_session_runtime_settings_without_session_fk(pool).await?;
    ensure_legacy_ai_model_config_columns_sqlite(pool)
        .await
        .ok();
    ensure_column(pool, "sessions", "status", "TEXT NOT NULL DEFAULT 'active'")
        .await
        .ok();
    ensure_column(pool, "sessions", "archived_at", "TEXT")
        .await
        .ok();
    ensure_column(pool, "agents", "task_runner_agent_account_id", "TEXT")
        .await
        .ok();
    ensure_column(
        pool,
        "chatos_contacts",
        "task_runner_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await
    .ok();
    ensure_column(pool, "chatos_contacts", "task_runner_base_url", "TEXT")
        .await
        .ok();
    ensure_column(
        pool,
        "chatos_contacts",
        "task_runner_agent_account_id",
        "TEXT",
    )
    .await
    .ok();
    ensure_column(pool, "chatos_contacts", "task_runner_username", "TEXT")
        .await
        .ok();
    ensure_column(pool, "chatos_contacts", "task_runner_password", "TEXT")
        .await
        .ok();
    ensure_column(pool, "terminals", "project_id", "TEXT")
        .await
        .ok();
    ensure_column(pool, "terminals", "kind", "TEXT NOT NULL DEFAULT 'shared'")
        .await
        .ok();
    ensure_column(pool, "remote_connections", "password", "TEXT")
        .await
        .ok();
    ensure_column(pool, "remote_connections", "jump_password", "TEXT")
        .await
        .ok();
    ensure_column(pool, "remote_connections", "jump_connection_id", "TEXT")
        .await
        .ok();
    ensure_column(pool, "remote_connections", "jump_certificate_path", "TEXT")
        .await
        .ok();
    ensure_column(
        pool,
        "project_run_environment_settings",
        "custom_toolchains_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )
    .await
    .ok();
    ensure_column(pool, "mcp_change_logs", "change_kind", "TEXT")
        .await
        .ok();
    ensure_column(pool, "mcp_change_logs", "project_id", "TEXT")
        .await
        .ok();
    ensure_column(
        pool,
        "mcp_change_logs",
        "confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await
    .ok();
    ensure_column(pool, "mcp_change_logs", "confirmed_at", "TEXT")
        .await
        .ok();
    ensure_column(pool, "mcp_change_logs", "confirmed_by", "TEXT")
        .await
        .ok();
    rename_column_if_needed(pool, "mcp_change_logs", "session_id", "conversation_id")
        .await
        .ok();
    rename_column_if_needed(pool, "task_manager_tasks", "session_id", "conversation_id")
        .await
        .ok();
    ensure_column(
        pool,
        "task_manager_tasks",
        "outcome_summary",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "task_manager_tasks",
        "outcome_items_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "task_manager_tasks",
        "resume_hint",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "task_manager_tasks",
        "blocker_reason",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "task_manager_tasks",
        "blocker_needs_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "task_manager_tasks",
        "blocker_kind",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await
    .ok();
    ensure_column(pool, "task_manager_tasks", "completed_at", "TEXT")
        .await
        .ok();
    ensure_column(pool, "task_manager_tasks", "last_outcome_at", "TEXT")
        .await
        .ok();
    ensure_column(
        pool,
        "project_run_environment_settings",
        "terminal_ui_enabled",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await
    .ok();
    rename_column_if_needed(
        pool,
        "ask_user_prompt_requests",
        "session_id",
        "conversation_id",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "ask_user_prompt_requests",
        "source",
        "TEXT NOT NULL DEFAULT 'chatos'",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "ask_user_prompt_requests",
        "external_prompt_id",
        "TEXT",
    )
    .await
    .ok();
    ensure_column(pool, "ask_user_prompt_requests", "external_task_id", "TEXT")
        .await
        .ok();
    ensure_column(pool, "ask_user_prompt_requests", "external_run_id", "TEXT")
        .await
        .ok();
    ensure_column(
        pool,
        "ask_user_prompt_requests",
        "external_project_id",
        "TEXT",
    )
    .await
    .ok();
    sqlx::query("DROP INDEX IF EXISTS idx_mcp_change_logs_session_id")
        .execute(pool)
        .await
        .ok();
    cleanup_project_agent_links_sqlite(pool).await?;
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_chatos_project_agent_links_user_project_unique \
        ON chatos_project_agent_links(user_id, project_id)",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("create project contact unique index failed: {e}"))?;

    for sql in statements::INDEXES {
        let _ = sqlx::query(sql).execute(pool).await;
    }

    Ok(())
}

async fn cleanup_project_agent_links_sqlite(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query(
        "DELETE FROM chatos_project_agent_links \
        WHERE status != 'active' OR contact_id IS NULL OR trim(contact_id) = ''",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("cleanup inactive project contact links failed: {e}"))?;

    sqlx::query(
        "DELETE FROM chatos_project_agent_links \
        WHERE rowid NOT IN ( \
            SELECT rowid FROM ( \
                SELECT rowid, row_number() OVER ( \
                    PARTITION BY user_id, project_id \
                    ORDER BY last_bound_at DESC, updated_at DESC, created_at DESC, rowid DESC \
                ) AS rank \
                FROM chatos_project_agent_links \
            ) ranked \
            WHERE rank = 1 \
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("dedupe project contact links failed: {e}"))?;

    Ok(())
}

async fn ensure_session_runtime_settings_without_session_fk(
    pool: &SqlitePool,
) -> Result<(), String> {
    let table_exists = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='session_runtime_settings' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("read session_runtime_settings existence failed: {e}"))?
    .is_some();
    if !table_exists {
        return Ok(());
    }

    let rows = sqlx::query("PRAGMA foreign_key_list(session_runtime_settings)")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("read session_runtime_settings foreign keys failed: {e}"))?;
    let has_session_fk = rows.iter().any(|row| {
        let target_table: String = row.try_get("table").unwrap_or_default();
        let from_column: String = row.try_get("from").unwrap_or_default();
        target_table == "sessions" && from_column == "session_id"
    });
    if !has_session_fk {
        return Ok(());
    }

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| format!("acquire sqlite connection failed: {e}"))?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("disable sqlite foreign keys failed: {e}"))?;

    let migration_result = async {
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("begin session_runtime_settings migration failed: {e}"))?;
        sqlx::query("DROP TABLE IF EXISTS session_runtime_settings_new")
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("drop temp session_runtime_settings table failed: {e}"))?;
        sqlx::query(
            r#"CREATE TABLE session_runtime_settings_new (
                session_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                selected_model_id TEXT,
                selected_model_name TEXT,
                selected_thinking_level TEXT,
                remote_connection_id TEXT,
                workspace_root TEXT,
                reasoning_enabled INTEGER NOT NULL DEFAULT 0,
                plan_mode_enabled INTEGER NOT NULL DEFAULT 0,
                mcp_enabled INTEGER NOT NULL DEFAULT 1,
                enabled_mcp_ids TEXT NOT NULL DEFAULT '[]',
                auto_create_task INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"#,
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("create temp session_runtime_settings table failed: {e}"))?;
        sqlx::query(
            r#"INSERT INTO session_runtime_settings_new (
                session_id,
                user_id,
                selected_model_id,
                selected_model_name,
                selected_thinking_level,
                remote_connection_id,
                workspace_root,
                reasoning_enabled,
                plan_mode_enabled,
                mcp_enabled,
                enabled_mcp_ids,
                auto_create_task,
                created_at,
                updated_at
            )
            SELECT
                session_id,
                user_id,
                selected_model_id,
                selected_model_name,
                selected_thinking_level,
                remote_connection_id,
                workspace_root,
                COALESCE(reasoning_enabled, 0),
                COALESCE(plan_mode_enabled, 0),
                mcp_enabled,
                enabled_mcp_ids,
                auto_create_task,
                created_at,
                updated_at
            FROM session_runtime_settings"#,
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("copy session_runtime_settings rows failed: {e}"))?;
        sqlx::query("DROP TABLE session_runtime_settings")
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("drop old session_runtime_settings table failed: {e}"))?;
        sqlx::query("ALTER TABLE session_runtime_settings_new RENAME TO session_runtime_settings")
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("rename session_runtime_settings table failed: {e}"))?;
        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("commit session_runtime_settings migration failed: {e}"))?;
        Ok::<(), String>(())
    }
    .await;

    if migration_result.is_err() {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
    }
    let restore_result = sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("restore sqlite foreign keys failed: {e}"));

    migration_result?;
    restore_result?;
    Ok(())
}

async fn ensure_legacy_ai_model_config_columns_sqlite(pool: &SqlitePool) -> Result<(), String> {
    let table_exists = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='ai_model_configs' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("read ai_model_configs existence failed: {e}"))?
    .is_some();
    if !table_exists {
        return Ok(());
    }

    let rows = sqlx::query("PRAGMA table_info(ai_model_configs)")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("read ai_model_configs columns failed: {e}"))?;
    let mut cols = HashSet::new();
    for row in rows {
        let name: String = row.try_get("name").unwrap_or_default();
        if !name.is_empty() {
            cols.insert(name);
        }
    }
    if !cols.contains("thinking_level") {
        sqlx::query("ALTER TABLE ai_model_configs ADD COLUMN thinking_level TEXT")
            .execute(pool)
            .await
            .map_err(|e| format!("add thinking_level column failed: {e}"))?;
    }
    if !cols.contains("supports_images") {
        sqlx::query("ALTER TABLE ai_model_configs ADD COLUMN supports_images INTEGER DEFAULT 0")
            .execute(pool)
            .await
            .map_err(|e| format!("add supports_images column failed: {e}"))?;
    }
    if !cols.contains("supports_reasoning") {
        sqlx::query("ALTER TABLE ai_model_configs ADD COLUMN supports_reasoning INTEGER DEFAULT 0")
            .execute(pool)
            .await
            .map_err(|e| format!("add supports_reasoning column failed: {e}"))?;
    }
    if !cols.contains("supports_responses") {
        sqlx::query("ALTER TABLE ai_model_configs ADD COLUMN supports_responses INTEGER DEFAULT 0")
            .execute(pool)
            .await
            .map_err(|e| format!("add supports_responses column failed: {e}"))?;
    }
    ensure_column(pool, "terminals", "process_id", "INTEGER").await?;
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_ai_model_configs_user_id ON ai_model_configs(user_id)",
    )
    .execute(pool)
    .await;
    Ok(())
}

async fn ensure_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    ddl: &str,
) -> Result<(), String> {
    let rows = sqlx::query(&format!("PRAGMA table_info({})", table))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut exists = false;
    for row in rows {
        let name: String = row.try_get("name").unwrap_or_default();
        if name == column {
            exists = true;
            break;
        }
    }
    if !exists {
        let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, ddl);
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn rename_column_if_needed(
    pool: &SqlitePool,
    table: &str,
    from_column: &str,
    to_column: &str,
) -> Result<(), String> {
    let rows = sqlx::query(&format!("PRAGMA table_info({})", table))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut from_exists = false;
    let mut to_exists = false;
    for row in rows {
        let name: String = row.try_get("name").unwrap_or_default();
        if name == from_column {
            from_exists = true;
        }
        if name == to_column {
            to_exists = true;
        }
    }

    if from_exists && !to_exists {
        let sql = format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            table, from_column, to_column
        );
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

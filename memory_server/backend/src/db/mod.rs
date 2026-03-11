use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::Row;
use sqlx::SqlitePool;
use tracing::info;

use crate::config::AppConfig;

pub async fn init_pool(config: &AppConfig) -> Result<SqlitePool, String> {
    ensure_parent_dir(config.database_url.as_str())?;

    let options = config
        .database_url
        .parse::<SqliteConnectOptions>()
        .map_err(|e| format!("invalid sqlite url: {e}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);

    SqlitePool::connect_with(options)
        .await
        .map_err(|e| e.to_string())
}

pub async fn init_schema(pool: &SqlitePool) -> Result<(), String> {
    let statements = vec![
        r#"CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            project_id TEXT,
            title TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            archived_at TEXT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            message_mode TEXT,
            message_source TEXT,
            tool_calls TEXT,
            tool_call_id TEXT,
            reasoning TEXT,
            metadata TEXT,
            summary_status TEXT NOT NULL DEFAULT 'pending',
            summary_id TEXT,
            summarized_at TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS session_summaries_v2 (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            summary_text TEXT NOT NULL,
            summary_model TEXT NOT NULL,
            trigger_type TEXT NOT NULL,
            source_start_message_id TEXT,
            source_end_message_id TEXT,
            source_message_count INTEGER NOT NULL DEFAULT 0,
            source_estimated_tokens INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'done',
            error_message TEXT,
            level INTEGER NOT NULL DEFAULT 0,
            rollup_status TEXT NOT NULL DEFAULT 'pending',
            rollup_summary_id TEXT,
            rolled_up_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS ai_model_configs (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            base_url TEXT,
            api_key TEXT,
            supports_images INTEGER NOT NULL DEFAULT 0,
            supports_reasoning INTEGER NOT NULL DEFAULT 0,
            supports_responses INTEGER NOT NULL DEFAULT 0,
            temperature REAL,
            thinking_level TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS auth_users (
            user_id TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'user',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS summary_job_configs (
            user_id TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 1,
            summary_model_config_id TEXT,
            token_limit INTEGER NOT NULL DEFAULT 6000,
            round_limit INTEGER NOT NULL DEFAULT 8,
            target_summary_tokens INTEGER NOT NULL DEFAULT 700,
            job_interval_seconds INTEGER NOT NULL DEFAULT 30,
            max_sessions_per_tick INTEGER NOT NULL DEFAULT 50,
            updated_at TEXT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS summary_rollup_job_configs (
            user_id TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 1,
            summary_model_config_id TEXT,
            token_limit INTEGER NOT NULL DEFAULT 6000,
            round_limit INTEGER NOT NULL DEFAULT 50,
            target_summary_tokens INTEGER NOT NULL DEFAULT 700,
            job_interval_seconds INTEGER NOT NULL DEFAULT 60,
            keep_raw_level0_count INTEGER NOT NULL DEFAULT 5,
            max_level INTEGER NOT NULL DEFAULT 4,
            max_sessions_per_tick INTEGER NOT NULL DEFAULT 50,
            updated_at TEXT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS job_runs (
            id TEXT PRIMARY KEY,
            job_type TEXT NOT NULL,
            session_id TEXT,
            status TEXT NOT NULL,
            trigger_type TEXT,
            input_count INTEGER NOT NULL DEFAULT 0,
            output_count INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            started_at TEXT NOT NULL,
            finished_at TEXT
        )"#,
    ];

    for sql in statements {
        sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| format!("create table failed: {e}"))?;
    }

    ensure_column_exists(
        pool,
        "ai_model_configs",
        "supports_images",
        "ALTER TABLE ai_model_configs ADD COLUMN supports_images INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column_exists(
        pool,
        "ai_model_configs",
        "supports_reasoning",
        "ALTER TABLE ai_model_configs ADD COLUMN supports_reasoning INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    let indexes = vec![
        "CREATE INDEX IF NOT EXISTS idx_sessions_user_status_created_at ON sessions(user_id, status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_sessions_project_status_created_at ON sessions(project_id, status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_messages_session_created_at ON messages(session_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_messages_session_summary_status_created_at ON messages(session_id, summary_status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_messages_summary_id ON messages(summary_id)",
        "CREATE INDEX IF NOT EXISTS idx_summaries_session_created_at ON session_summaries_v2(session_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_summaries_session_status_created_at ON session_summaries_v2(session_id, status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_summaries_rollup_scan ON session_summaries_v2(session_id, level, status, rollup_status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_summaries_rollup_summary_id ON session_summaries_v2(rollup_summary_id)",
        "CREATE INDEX IF NOT EXISTS idx_model_configs_user_enabled_updated_at ON ai_model_configs(user_id, enabled, updated_at)",
        "CREATE INDEX IF NOT EXISTS idx_auth_users_role ON auth_users(role)",
        "CREATE INDEX IF NOT EXISTS idx_job_runs_type_started_at ON job_runs(job_type, started_at)",
        "CREATE INDEX IF NOT EXISTS idx_job_runs_session_started_at ON job_runs(session_id, started_at)",
    ];

    for index_sql in indexes {
        let _ = sqlx::query(index_sql).execute(pool).await;
    }

    info!("[MEMORY-SERVER] sqlite schema initialized");
    Ok(())
}

async fn ensure_column_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<(), String> {
    let pragma_sql = format!("PRAGMA table_info({})", table);
    let rows = sqlx::query(pragma_sql.as_str())
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let exists = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    });

    if !exists {
        sqlx::query(alter_sql)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn ensure_parent_dir(database_url: &str) -> Result<(), String> {
    if !database_url.starts_with("sqlite://") {
        return Ok(());
    }

    let raw_path = database_url.trim_start_matches("sqlite://");
    if raw_path == ":memory:" || raw_path.is_empty() {
        return Ok(());
    }

    let path = Path::new(raw_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

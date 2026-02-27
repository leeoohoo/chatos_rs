use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use tracing::info;

use super::types::SqliteConfig;

pub(super) async fn init_sqlite(cfg: &SqliteConfig) -> Result<SqlitePool, String> {
    let db_path = cfg
        .db_path
        .clone()
        .unwrap_or_else(|| "data/chat_app.db".to_string());
    let path = Path::new(&db_path);
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create sqlite dir failed: {e}"))?;
        }
    }

    let mut options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    if let Some(timeout) = cfg.timeout {
        options = options.busy_timeout(Duration::from_millis(timeout));
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| format!("sqlite connect failed: {e}"))?;

    sqlx::query("PRAGMA synchronous = NORMAL")
        .execute(&pool)
        .await
        .ok();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .ok();
    if let Some(busy) = cfg.busy_timeout {
        let _ = sqlx::query(&format!("PRAGMA busy_timeout = {}", busy))
            .execute(&pool)
            .await;
    }

    create_tables_sqlite(&pool).await?;

    info!("[SQLite] database initialized: {}", db_path);
    Ok(pool)
}

async fn create_tables_sqlite(pool: &SqlitePool) -> Result<(), String> {
    let statements = vec![
        r#"CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT,
            metadata TEXT,
            user_id TEXT,
            project_id TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            message_mode TEXT,
            message_source TEXT,
            summary TEXT,
            tool_calls TEXT,
            tool_call_id TEXT,
            reasoning TEXT,
            metadata TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS session_summaries (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            summary_text TEXT NOT NULL,
            summary_prompt TEXT,
            model TEXT,
            temperature REAL,
            target_summary_tokens INTEGER,
            keep_last_n INTEGER,
            message_count INTEGER,
            approx_tokens INTEGER,
            first_message_id TEXT,
            last_message_id TEXT,
            first_message_created_at TEXT,
            last_message_created_at TEXT,
            metadata TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
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
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS session_summary_messages (
            id TEXT PRIMARY KEY,
            summary_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (summary_id) REFERENCES session_summaries(id) ON DELETE CASCADE,
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS mcp_configs (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            command TEXT NOT NULL,
            type TEXT DEFAULT 'stdio',
            args TEXT,
            env TEXT,
            cwd TEXT,
            user_id TEXT,
            enabled INTEGER DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS mcp_change_logs (
            id TEXT PRIMARY KEY,
            server_name TEXT NOT NULL,
            path TEXT NOT NULL,
            action TEXT NOT NULL,
            bytes INTEGER NOT NULL,
            sha256 TEXT,
            diff TEXT,
            session_id TEXT,
            run_id TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS task_manager_tasks (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            conversation_turn_id TEXT NOT NULL,
            title TEXT NOT NULL,
            details TEXT NOT NULL,
            priority TEXT NOT NULL,
            status TEXT NOT NULL,
            tags_json TEXT NOT NULL,
            due_at TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS mcp_config_profiles (
            id TEXT PRIMARY KEY,
            mcp_config_id TEXT NOT NULL,
            name TEXT NOT NULL,
            args TEXT,
            env TEXT,
            cwd TEXT,
            enabled INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (mcp_config_id) REFERENCES mcp_configs(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS ai_model_configs (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider TEXT DEFAULT 'openai',
            model TEXT NOT NULL,
            thinking_level TEXT,
            api_key TEXT,
            base_url TEXT,
            user_id TEXT,
            enabled INTEGER DEFAULT 1,
            supports_images INTEGER DEFAULT 0,
            supports_reasoning INTEGER DEFAULT 0,
            supports_responses INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS system_contexts (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            content TEXT,
            user_id TEXT NOT NULL,
            is_active INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            ai_model_config_id TEXT NOT NULL,
            system_context_id TEXT,
            description TEXT,
            user_id TEXT,
            mcp_config_ids TEXT,
            callable_agent_ids TEXT,
            project_id TEXT,
            workspace_dir TEXT,
            enabled INTEGER DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (ai_model_config_id) REFERENCES ai_model_configs(id),
            FOREIGN KEY (system_context_id) REFERENCES system_contexts(id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS applications (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            description TEXT,
            user_id TEXT,
            enabled INTEGER DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            root_path TEXT NOT NULL,
            description TEXT,
            user_id TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS terminals (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            cwd TEXT NOT NULL,
            user_id TEXT,
            project_id TEXT,
            status TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            last_active_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS terminal_logs (
            id TEXT PRIMARY KEY,
            terminal_id TEXT NOT NULL,
            type TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (terminal_id) REFERENCES terminals(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS mcp_config_applications (
            id TEXT PRIMARY KEY,
            mcp_config_id TEXT NOT NULL,
            application_id TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (mcp_config_id) REFERENCES mcp_configs(id) ON DELETE CASCADE,
            FOREIGN KEY (application_id) REFERENCES applications(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS system_context_applications (
            id TEXT PRIMARY KEY,
            system_context_id TEXT NOT NULL,
            application_id TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (system_context_id) REFERENCES system_contexts(id) ON DELETE CASCADE,
            FOREIGN KEY (application_id) REFERENCES applications(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS agent_applications (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            application_id TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE,
            FOREIGN KEY (application_id) REFERENCES applications(id) ON DELETE CASCADE
        )"#,
        r#"CREATE TABLE IF NOT EXISTS session_mcp_servers (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            mcp_server_name TEXT,
            mcp_config_id TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
            FOREIGN KEY (mcp_config_id) REFERENCES mcp_configs(id)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS user_settings (
            user_id TEXT PRIMARY KEY,
            settings TEXT,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS session_summary_job_configs (
            user_id TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 1,
            summary_model_config_id TEXT,
            token_limit INTEGER NOT NULL DEFAULT 6000,
            round_limit INTEGER NOT NULL DEFAULT 8,
            target_summary_tokens INTEGER NOT NULL DEFAULT 700,
            job_interval_seconds INTEGER NOT NULL DEFAULT 30,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
    ];

    for sql in statements {
        sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| format!("create table failed: {e}"))?;
    }

    ensure_ai_model_config_columns_sqlite(pool).await?;

    ensure_column(
        pool,
        "ai_model_configs",
        "supports_images",
        "INTEGER DEFAULT 0",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "ai_model_configs",
        "supports_reasoning",
        "INTEGER DEFAULT 0",
    )
    .await
    .ok();
    ensure_column(
        pool,
        "ai_model_configs",
        "supports_responses",
        "INTEGER DEFAULT 0",
    )
    .await
    .ok();
    ensure_column(pool, "agents", "mcp_config_ids", "TEXT")
        .await
        .ok();
    ensure_column(pool, "agents", "callable_agent_ids", "TEXT")
        .await
        .ok();
    ensure_column(pool, "agents", "project_id", "TEXT")
        .await
        .ok();
    ensure_column(pool, "agents", "workspace_dir", "TEXT")
        .await
        .ok();
    ensure_column(pool, "terminals", "project_id", "TEXT")
        .await
        .ok();
    ensure_column(pool, "messages", "message_mode", "TEXT")
        .await
        .ok();
    ensure_column(pool, "messages", "message_source", "TEXT")
        .await
        .ok();
    ensure_column(
        pool,
        "messages",
        "summary_status",
        "TEXT NOT NULL DEFAULT 'pending'",
    )
    .await
    .ok();
    ensure_column(pool, "messages", "summary_id", "TEXT")
        .await
        .ok();
    ensure_column(pool, "messages", "summarized_at", "TEXT")
        .await
        .ok();

    let indexes = vec![
        "CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_messages_message_mode ON messages(message_mode)",
        "CREATE INDEX IF NOT EXISTS idx_messages_message_source ON messages(message_source)",
        "CREATE INDEX IF NOT EXISTS idx_messages_summary_status ON messages(summary_status)",
        "CREATE INDEX IF NOT EXISTS idx_messages_session_summary_status_created_at ON messages(session_id, summary_status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_messages_summary_id ON messages(summary_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summaries_session_id ON session_summaries(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summaries_last_created_at ON session_summaries(session_id, last_message_created_at)",
        "CREATE INDEX IF NOT EXISTS idx_session_summaries_v2_session_id ON session_summaries_v2(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summaries_v2_session_created_at ON session_summaries_v2(session_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_session_summaries_v2_session_status_created_at ON session_summaries_v2(session_id, status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_session_summary_messages_session_id ON session_summary_messages(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summary_messages_summary_id ON session_summary_messages(summary_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summary_messages_message_id ON session_summary_messages(message_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summary_job_configs_user_id ON session_summary_job_configs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_server_name ON mcp_change_logs(server_name)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_session_id ON mcp_change_logs(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_created_at ON mcp_change_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_task_manager_tasks_session_turn ON task_manager_tasks(session_id, conversation_turn_id)",
        "CREATE INDEX IF NOT EXISTS idx_task_manager_tasks_session_created_at ON task_manager_tasks(session_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_task_manager_tasks_turn_created_at ON task_manager_tasks(conversation_turn_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_sessions_project_id ON sessions(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_configs_user_id ON mcp_configs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_configs_enabled ON mcp_configs(enabled)",
        "CREATE INDEX IF NOT EXISTS idx_ai_model_configs_user_id ON ai_model_configs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_system_contexts_user_id ON system_contexts(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_agents_user_id ON agents(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_agents_project_id ON agents(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_applications_user_id ON applications(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_projects_user_id ON projects(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminals_user_id ON terminals(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminals_project_id ON terminals(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminals_status ON terminals(status)",
        "CREATE INDEX IF NOT EXISTS idx_terminal_logs_terminal_id ON terminal_logs(terminal_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminal_logs_created_at ON terminal_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_session_mcp_servers_session_id ON session_mcp_servers(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_config_profiles_mcp_config_id ON mcp_config_profiles(mcp_config_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_config_applications_mcp_config_id ON mcp_config_applications(mcp_config_id)",
        "CREATE INDEX IF NOT EXISTS idx_system_context_applications_context_id ON system_context_applications(system_context_id)",
        "CREATE INDEX IF NOT EXISTS idx_agent_applications_agent_id ON agent_applications(agent_id)",
    ];
    for sql in indexes {
        let _ = sqlx::query(sql).execute(pool).await;
    }

    Ok(())
}

async fn ensure_ai_model_config_columns_sqlite(pool: &SqlitePool) -> Result<(), String> {
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

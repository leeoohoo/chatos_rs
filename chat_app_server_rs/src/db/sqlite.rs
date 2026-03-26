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
            status TEXT NOT NULL DEFAULT 'active',
            archived_at TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
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
            project_id TEXT,
            path TEXT NOT NULL,
            action TEXT NOT NULL,
            change_kind TEXT,
            bytes INTEGER NOT NULL,
            sha256 TEXT,
            diff TEXT,
            session_id TEXT,
            run_id TEXT,
            confirmed INTEGER NOT NULL DEFAULT 0,
            confirmed_at TEXT,
            confirmed_by TEXT,
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
        r#"CREATE TABLE IF NOT EXISTS ui_prompt_requests (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            conversation_turn_id TEXT NOT NULL,
            tool_call_id TEXT,
            kind TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            prompt_json TEXT NOT NULL,
            response_json TEXT,
            expires_at TEXT,
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
        r#"CREATE TABLE IF NOT EXISTS remote_connections (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            host TEXT NOT NULL,
            port INTEGER NOT NULL DEFAULT 22,
            username TEXT NOT NULL,
            auth_type TEXT NOT NULL DEFAULT 'private_key',
            password TEXT,
            private_key_path TEXT,
            certificate_path TEXT,
            default_remote_path TEXT,
            host_key_policy TEXT NOT NULL DEFAULT 'strict',
            jump_enabled INTEGER NOT NULL DEFAULT 0,
            jump_host TEXT,
            jump_port INTEGER,
            jump_username TEXT,
            jump_private_key_path TEXT,
            jump_password TEXT,
            user_id TEXT,
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
    ensure_column(pool, "sessions", "status", "TEXT NOT NULL DEFAULT 'active'")
        .await
        .ok();
    ensure_column(pool, "sessions", "archived_at", "TEXT")
        .await
        .ok();
    ensure_column(pool, "terminals", "project_id", "TEXT")
        .await
        .ok();
    ensure_column(pool, "remote_connections", "password", "TEXT")
        .await
        .ok();
    ensure_column(pool, "remote_connections", "jump_password", "TEXT")
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

    let indexes = vec![
        "CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status)",
        "CREATE INDEX IF NOT EXISTS idx_sessions_user_status_created_at ON sessions(user_id, status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_server_name ON mcp_change_logs(server_name)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_project_id ON mcp_change_logs(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_session_id ON mcp_change_logs(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_created_at ON mcp_change_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_confirmed_created_at ON mcp_change_logs(confirmed, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_path ON mcp_change_logs(path)",
        "CREATE INDEX IF NOT EXISTS idx_task_manager_tasks_session_turn ON task_manager_tasks(session_id, conversation_turn_id)",
        "CREATE INDEX IF NOT EXISTS idx_task_manager_tasks_session_created_at ON task_manager_tasks(session_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_task_manager_tasks_turn_created_at ON task_manager_tasks(conversation_turn_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_ui_prompt_requests_session_status_updated_at ON ui_prompt_requests(session_id, status, updated_at)",
        "CREATE INDEX IF NOT EXISTS idx_ui_prompt_requests_turn_created_at ON ui_prompt_requests(conversation_turn_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_sessions_project_id ON sessions(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_configs_user_id ON mcp_configs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_configs_enabled ON mcp_configs(enabled)",
        "CREATE INDEX IF NOT EXISTS idx_ai_model_configs_user_id ON ai_model_configs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_system_contexts_user_id ON system_contexts(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_applications_user_id ON applications(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_projects_user_id ON projects(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminals_user_id ON terminals(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminals_project_id ON terminals(project_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminals_status ON terminals(status)",
        "CREATE INDEX IF NOT EXISTS idx_remote_connections_user_id ON remote_connections(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_remote_connections_host ON remote_connections(host)",
        "CREATE INDEX IF NOT EXISTS idx_terminal_logs_terminal_id ON terminal_logs(terminal_id)",
        "CREATE INDEX IF NOT EXISTS idx_terminal_logs_terminal_created_at ON terminal_logs(terminal_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_terminal_logs_created_at ON terminal_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_session_mcp_servers_session_id ON session_mcp_servers(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_config_profiles_mcp_config_id ON mcp_config_profiles(mcp_config_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_config_applications_mcp_config_id ON mcp_config_applications(mcp_config_id)",
        "CREATE INDEX IF NOT EXISTS idx_system_context_applications_context_id ON system_context_applications(system_context_id)",
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

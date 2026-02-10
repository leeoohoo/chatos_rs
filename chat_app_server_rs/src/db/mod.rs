#![allow(dead_code)]
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

use mongodb::bson::doc;
use mongodb::options::{ClientOptions, ResolverConfig};
use mongodb::{Client, Database as MongoDatabase, IndexModel};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};

static DB_FACTORY: OnceCell<Arc<DatabaseFactory>> = OnceCell::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    Sqlite,
    Mongodb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    pub db_path: Option<String>,
    pub timeout: Option<u64>,
    pub busy_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub connection_string: Option<String>,
    pub max_pool_size: Option<u32>,
    pub min_pool_size: Option<u32>,
    pub server_selection_timeout_ms: Option<u64>,
    pub connect_timeout_ms: Option<u64>,
    pub socket_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(rename = "type")]
    pub db_type: Option<DatabaseType>,
    pub sqlite: Option<SqliteConfig>,
    pub mongodb: Option<MongoConfig>,
    pub auto_migrate: Option<bool>,
    pub debug: Option<bool>,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            db_path: Some("data/chat_app.db".to_string()),
            timeout: Some(30000),
            busy_timeout: Some(30000),
        }
    }
}

impl Default for MongoConfig {
    fn default() -> Self {
        Self {
            host: Some("localhost".to_string()),
            port: Some(27017),
            database: Some("chat_app".to_string()),
            username: None,
            password: None,
            connection_string: None,
            max_pool_size: Some(100),
            min_pool_size: Some(0),
            server_selection_timeout_ms: Some(30000),
            connect_timeout_ms: Some(20000),
            socket_timeout_ms: Some(20000),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: Some(DatabaseType::Sqlite),
            sqlite: Some(SqliteConfig::default()),
            mongodb: Some(MongoConfig::default()),
            auto_migrate: Some(true),
            debug: Some(false),
        }
    }
}

pub enum Database {
    Sqlite(SqlitePool),
    Mongo { client: Client, db: MongoDatabase },
}

impl Database {
    pub fn is_mongo(&self) -> bool {
        matches!(self, Database::Mongo { .. })
    }

    pub fn sqlite_pool(&self) -> Option<&SqlitePool> {
        match self {
            Database::Sqlite(pool) => Some(pool),
            _ => None,
        }
    }

    pub fn mongo_db(&self) -> Option<&MongoDatabase> {
        match self {
            Database::Mongo { db, .. } => Some(db),
            _ => None,
        }
    }
}

struct DatabaseFactoryInner {
    adapter: Option<Arc<Database>>,
    config: Option<DatabaseConfig>,
}

pub struct DatabaseFactory {
    inner: Mutex<DatabaseFactoryInner>,
}

impl DatabaseFactory {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(DatabaseFactoryInner {
                adapter: None,
                config: None,
            }),
        }
    }

    pub async fn get_adapter(&self) -> Result<Arc<Database>, String> {
        let mut inner = self.inner.lock().await;
        if let Some(adapter) = inner.adapter.clone() {
            return Ok(adapter);
        }

        let config = self.load_config(None)?;
        let adapter = self.create_adapter(&config).await?;
        inner.config = Some(config);
        inner.adapter = Some(adapter.clone());
        Ok(adapter)
    }

    pub fn get_adapter_sync(&self) -> Result<Arc<Database>, String> {
        if tokio::runtime::Handle::try_current().is_ok() {
            if let Ok(inner) = self.inner.try_lock() {
                if let Some(adapter) = inner.adapter.clone() {
                    return Ok(adapter);
                }
                return Err(
                    "Database adapter not initialized. Call get_adapter() first.".to_string(),
                );
            }
            return Err(
                "Database adapter busy. Use async get_adapter() within runtime.".to_string(),
            );
        }

        let inner = self.inner.blocking_lock();
        if let Some(adapter) = inner.adapter.clone() {
            return Ok(adapter);
        }
        Err("Database adapter not initialized. Call get_adapter() first.".to_string())
    }

    pub fn load_config(&self, config_path: Option<PathBuf>) -> Result<DatabaseConfig, String> {
        let path = config_path.unwrap_or_else(|| PathBuf::from("config/database.json"));
        let mut cfg = if path.exists() {
            let raw =
                std::fs::read_to_string(&path).map_err(|e| format!("read config failed: {e}"))?;
            let trimmed = raw.trim_start_matches('\u{feff}').trim();
            if trimmed.is_empty() {
                warn!(
                    "[DatabaseFactory] config empty at {:?}, using default",
                    path
                );
                DatabaseConfig::default()
            } else {
                serde_json::from_str::<DatabaseConfig>(trimmed)
                    .map_err(|e| format!("parse config failed: {e}"))?
            }
        } else {
            warn!(
                "[DatabaseFactory] config not found at {:?}, using default",
                path
            );
            DatabaseConfig::default()
        };

        cfg = apply_env_overrides(cfg);
        Ok(cfg)
    }

    async fn create_adapter(&self, config: &DatabaseConfig) -> Result<Arc<Database>, String> {
        let db_type = config.db_type.clone().unwrap_or(DatabaseType::Sqlite);
        match db_type {
            DatabaseType::Sqlite => {
                let sqlite_cfg = config.sqlite.clone().unwrap_or_default();
                let db = init_sqlite(&sqlite_cfg).await?;
                Ok(Arc::new(Database::Sqlite(db)))
            }
            DatabaseType::Mongodb => {
                let mongo_cfg = config.mongodb.clone().unwrap_or_default();
                let db = init_mongodb(&mongo_cfg).await?;
                Ok(Arc::new(db))
            }
        }
    }

    pub async fn switch_database(
        &self,
        new_config: DatabaseConfig,
    ) -> Result<Arc<Database>, String> {
        let adapter = self.create_adapter(&new_config).await?;
        let mut inner = self.inner.lock().await;
        inner.adapter = Some(adapter.clone());
        inner.config = Some(new_config);
        Ok(adapter)
    }

    pub async fn switch_to_sqlite(&self, db_path: Option<String>) -> Result<Arc<Database>, String> {
        let cfg = DatabaseConfig {
            db_type: Some(DatabaseType::Sqlite),
            sqlite: Some(SqliteConfig {
                db_path,
                ..SqliteConfig::default()
            }),
            ..DatabaseConfig::default()
        };
        self.switch_database(cfg).await
    }

    pub async fn switch_to_mongodb(
        &self,
        host: Option<String>,
        port: Option<u16>,
        database: Option<String>,
    ) -> Result<Arc<Database>, String> {
        let cfg = DatabaseConfig {
            db_type: Some(DatabaseType::Mongodb),
            mongodb: Some(MongoConfig {
                host,
                port,
                database,
                ..MongoConfig::default()
            }),
            ..DatabaseConfig::default()
        };
        self.switch_database(cfg).await
    }
}

pub async fn init_global() -> Result<Arc<Database>, String> {
    let factory = Arc::new(DatabaseFactory::new());
    DB_FACTORY
        .set(factory.clone())
        .map_err(|_| "DB factory already initialized".to_string())?;
    factory.get_adapter().await
}

pub fn get_factory() -> Arc<DatabaseFactory> {
    DB_FACTORY
        .get()
        .expect("DB factory not initialized")
        .clone()
}

pub async fn get_db() -> Result<Arc<Database>, String> {
    get_factory().get_adapter().await
}

pub fn get_db_sync() -> Result<Arc<Database>, String> {
    get_factory().get_adapter_sync()
}

fn apply_env_overrides(mut cfg: DatabaseConfig) -> DatabaseConfig {
    let db_type_env = std::env::var("DATABASE_TYPE")
        .ok()
        .map(|s| s.trim().to_lowercase());
    if let Some(t) = db_type_env {
        if t == "sqlite" {
            cfg.db_type = Some(DatabaseType::Sqlite);
        } else if t == "mongodb" {
            cfg.db_type = Some(DatabaseType::Mongodb);
        }
    }

    let has_mongo_env = [
        "MONGODB_CONNECTION_STRING",
        "MONGODB_HOST",
        "MONGODB_PORT",
        "MONGODB_DB",
        "MONGODB_USER",
        "MONGODB_PASSWORD",
        "MONGODB_AUTH_SOURCE",
    ]
    .iter()
    .any(|k| {
        std::env::var(k)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    });

    if has_mongo_env {
        cfg.db_type = Some(DatabaseType::Mongodb);
        let mut mongo = cfg.mongodb.clone().unwrap_or_default();
        let host = std::env::var("MONGODB_HOST")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or(mongo.host.clone())
            .unwrap_or_else(|| "localhost".to_string());
        let port = std::env::var("MONGODB_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .or(mongo.port)
            .unwrap_or(27017);
        let database = std::env::var("MONGODB_DB")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or(mongo.database.clone())
            .unwrap_or_else(|| "chat_app".to_string());
        let username = std::env::var("MONGODB_USER")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or(mongo.username.clone());
        let password = std::env::var("MONGODB_PASSWORD")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or(mongo.password.clone());
        let auth_source = std::env::var("MONGODB_AUTH_SOURCE").ok();
        let conn_env = std::env::var("MONGODB_CONNECTION_STRING")
            .ok()
            .filter(|v| !v.trim().is_empty());

        mongo.host = Some(host.clone());
        mongo.port = Some(port);
        mongo.database = Some(database.clone());
        mongo.username = username.clone();
        mongo.password = password.clone();

        if let Some(conn) = conn_env {
            mongo.connection_string = Some(conn);
        } else {
            let cred = if let (Some(u), Some(p)) = (username.clone(), password.clone()) {
                format!("{}:{}@", urlencoding::encode(&u), urlencoding::encode(&p))
            } else {
                "".to_string()
            };
            let auth_query = auth_source
                .map(|a| format!("?authSource={}", urlencoding::encode(&a)))
                .unwrap_or_default();
            mongo.connection_string = Some(format!(
                "mongodb://{}{}:{}/{}{}",
                cred, host, port, database, auth_query
            ));
        }

        cfg.mongodb = Some(mongo);
    }

    cfg
}

async fn init_sqlite(cfg: &SqliteConfig) -> Result<SqlitePool, String> {
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

    let indexes = vec![
        "CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_session_summaries_session_id ON session_summaries(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summaries_last_created_at ON session_summaries(session_id, last_message_created_at)",
        "CREATE INDEX IF NOT EXISTS idx_session_summary_messages_session_id ON session_summary_messages(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summary_messages_summary_id ON session_summary_messages(summary_id)",
        "CREATE INDEX IF NOT EXISTS idx_session_summary_messages_message_id ON session_summary_messages(message_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_server_name ON mcp_change_logs(server_name)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_session_id ON mcp_change_logs(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_mcp_change_logs_created_at ON mcp_change_logs(created_at)",
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

async fn init_mongodb(cfg: &MongoConfig) -> Result<Database, String> {
    let connection_string = if let Some(conn) = cfg.connection_string.clone() {
        conn
    } else {
        let host = cfg.host.clone().unwrap_or_else(|| "localhost".to_string());
        let port = cfg.port.unwrap_or(27017);
        let database = cfg
            .database
            .clone()
            .unwrap_or_else(|| "chat_app".to_string());
        let cred = match (&cfg.username, &cfg.password) {
            (Some(u), Some(p)) => format!("{}:{}@", urlencoding::encode(u), urlencoding::encode(p)),
            _ => "".to_string(),
        };
        format!("mongodb://{}{}:{}/{}", cred, host, port, database)
    };

    let mut options =
        ClientOptions::parse_with_resolver_config(&connection_string, ResolverConfig::cloudflare())
            .await
            .map_err(|e| format!("mongodb parse options failed: {e}"))?;
    if let Some(max_pool) = cfg.max_pool_size {
        options.max_pool_size = Some(max_pool);
    }
    if let Some(min_pool) = cfg.min_pool_size {
        options.min_pool_size = Some(min_pool);
    }
    if let Some(ms) = cfg.server_selection_timeout_ms {
        options.server_selection_timeout = Some(Duration::from_millis(ms));
    }
    if let Some(ms) = cfg.connect_timeout_ms {
        options.connect_timeout = Some(Duration::from_millis(ms));
    }
    let _ = cfg.socket_timeout_ms;

    let client =
        Client::with_options(options).map_err(|e| format!("mongodb client failed: {e}"))?;
    let db_name = cfg
        .database
        .clone()
        .unwrap_or_else(|| "chat_app".to_string());
    let db = client.database(&db_name);

    let collections = vec![
        "sessions",
        "messages",
        "session_summaries",
        "session_summary_messages",
        "mcp_configs",
        "mcp_change_logs",
        "mcp_config_profiles",
        "ai_model_configs",
        "system_contexts",
        "agents",
        "applications",
        "projects",
        "terminals",
        "terminal_logs",
        "mcp_config_applications",
        "system_context_applications",
        "agent_applications",
        "session_mcp_servers",
        "user_settings",
    ];
    let existing = db
        .list_collection_names(None)
        .await
        .map_err(|e| e.to_string())?;
    for name in collections {
        if !existing.contains(&name.to_string()) {
            let _ = db.create_collection(name, None).await;
        }
    }

    let _ = db
        .collection::<mongodb::bson::Document>("sessions")
        .create_index(
            IndexModel::builder().keys(doc! { "user_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("sessions")
        .create_index(
            IndexModel::builder().keys(doc! { "project_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("messages")
        .create_index(
            IndexModel::builder().keys(doc! { "session_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("session_summaries")
        .create_index(
            IndexModel::builder().keys(doc! { "session_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("session_summaries")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "session_id": 1, "last_message_created_at": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("session_summary_messages")
        .create_index(
            IndexModel::builder().keys(doc! { "session_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("session_summary_messages")
        .create_index(
            IndexModel::builder().keys(doc! { "summary_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "server_name": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder().keys(doc! { "session_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder().keys(doc! { "created_at": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("agents")
        .create_index(
            IndexModel::builder().keys(doc! { "project_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("projects")
        .create_index(
            IndexModel::builder().keys(doc! { "user_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminals")
        .create_index(
            IndexModel::builder().keys(doc! { "user_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminals")
        .create_index(
            IndexModel::builder().keys(doc! { "status": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminal_logs")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "terminal_id": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminal_logs")
        .create_index(
            IndexModel::builder().keys(doc! { "created_at": 1 }).build(),
            None,
        )
        .await;

    Ok(Database::Mongo { client, db })
}

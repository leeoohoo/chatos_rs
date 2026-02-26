use mongodb::{Client, Database as MongoDatabase};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

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

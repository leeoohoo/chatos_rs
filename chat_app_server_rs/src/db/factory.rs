use std::path::PathBuf;
use std::sync::Arc;

use once_cell::sync::OnceCell;
use tokio::sync::Mutex;
use tracing::warn;

use super::mongodb::init_mongodb;
use super::sqlite::init_sqlite;
use super::types::{Database, DatabaseConfig, DatabaseType, MongoConfig, SqliteConfig};

static DB_FACTORY: OnceCell<Arc<DatabaseFactory>> = OnceCell::new();

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

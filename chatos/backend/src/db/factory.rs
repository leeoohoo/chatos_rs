// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;

use super::mongodb::init_mongodb;
use super::types::{Database, DatabaseConfig, DatabaseType, MongoConfig};
use once_cell::sync::OnceCell;
use tokio::sync::Mutex;

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

    pub fn load_config(&self, _config_path: Option<PathBuf>) -> Result<DatabaseConfig, String> {
        let connection_string = require_managed_database_value("MONGODB_CONNECTION_STRING")?;
        let database = require_managed_database_value("MONGODB_DB")?;
        Ok(build_managed_database_config(connection_string, database))
    }

    async fn create_adapter(&self, config: &DatabaseConfig) -> Result<Arc<Database>, String> {
        let mongo_cfg = config.mongodb.clone().unwrap_or_default();
        let db = init_mongodb(&mongo_cfg).await?;
        Ok(Arc::new(db))
    }
}

fn require_managed_database_value(key: &str) -> Result<String, String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required from configuration center"))
}

fn build_managed_database_config(connection_string: String, database: String) -> DatabaseConfig {
    let defaults = MongoConfig::default();
    DatabaseConfig {
        db_type: Some(DatabaseType::Mongodb),
        mongodb: Some(MongoConfig {
            host: None,
            port: None,
            database: Some(database),
            username: None,
            password: None,
            connection_string: Some(connection_string),
            max_pool_size: defaults.max_pool_size,
            min_pool_size: defaults.min_pool_size,
            server_selection_timeout_ms: defaults.server_selection_timeout_ms,
            connect_timeout_ms: defaults.connect_timeout_ms,
            socket_timeout_ms: defaults.socket_timeout_ms,
        }),
        auto_migrate: None,
        debug: None,
    }
}

pub async fn init_global() -> Result<Arc<Database>, String> {
    let factory = Arc::new(DatabaseFactory::new());
    DB_FACTORY
        .set(factory.clone())
        .map_err(|_| "DB factory already initialized".to_string())?;
    factory.get_adapter().await
}

pub fn get_factory() -> Result<Arc<DatabaseFactory>, String> {
    DB_FACTORY
        .get()
        .cloned()
        .ok_or_else(|| "DB factory not initialized".to_string())
}

pub async fn get_db() -> Result<Arc<Database>, String> {
    get_factory()?.get_adapter().await
}

pub fn get_db_sync() -> Result<Arc<Database>, String> {
    get_factory()?.get_adapter_sync()
}

#[cfg(test)]
mod tests {
    use super::build_managed_database_config;
    use crate::db::types::DatabaseType;

    #[test]
    fn managed_database_config_uses_only_authoritative_connection_values() {
        let cfg = build_managed_database_config(
            "mongodb://managed.example:27017/managed".to_string(),
            "managed".to_string(),
        );

        assert!(matches!(cfg.db_type, Some(DatabaseType::Mongodb)));
        let mongo = cfg.mongodb.expect("mongodb config");
        assert_eq!(mongo.database.as_deref(), Some("managed"));
        assert_eq!(
            mongo.connection_string.as_deref(),
            Some("mongodb://managed.example:27017/managed"),
        );
        assert_eq!(mongo.host, None);
        assert_eq!(mongo.port, None);
        assert_eq!(mongo.username, None);
        assert_eq!(mongo.password, None);
    }
}

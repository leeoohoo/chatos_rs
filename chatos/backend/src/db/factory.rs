// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use once_cell::sync::OnceCell;
use tokio::sync::Mutex;

use super::types::Database;

static DB_FACTORY: OnceCell<Arc<DatabaseFactory>> = OnceCell::new();
static DATABASE_POOL: OnceCell<chatos_postgres::PgPool> = OnceCell::new();

pub struct DatabaseFactory {
    adapter: Mutex<Option<Arc<Database>>>,
}

impl DatabaseFactory {
    pub fn new() -> Self {
        Self {
            adapter: Mutex::new(None),
        }
    }

    pub async fn get_adapter(&self) -> Result<Arc<Database>, String> {
        let mut adapter = self.adapter.lock().await;
        if let Some(database) = adapter.clone() {
            return Ok(database);
        }
        let database_url = required_database_url()?;
        let config =
            chatos_postgres::PostgresConfig::from_env(database_url, "chatos-backend", "CHATOS")
                .map_err(|error| error.to_string())?;
        let pool = chatos_postgres::connect(&config)
            .await
            .map_err(|error| error.to_string())?;
        let _ = DATABASE_POOL.set(pool.clone());
        let database = Arc::new(Database { pool });
        *adapter = Some(database.clone());
        Ok(database)
    }
}

fn required_database_url() -> Result<String, String> {
    std::env::var("CHATOS_DATABASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "CHATOS_DATABASE_URL is required from configuration center".to_string())
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

pub fn get_pool() -> Result<&'static chatos_postgres::PgPool, String> {
    DATABASE_POOL
        .get()
        .ok_or_else(|| "DB pool is not initialized".to_string())
}

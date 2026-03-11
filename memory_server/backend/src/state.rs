use sqlx::SqlitePool;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: AppConfig,
}

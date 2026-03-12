use crate::config::AppConfig;
use crate::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub config: AppConfig,
}

use crate::config::AppConfig;
use crate::db::Db;
use crate::event_hub::SharedImEventHub;

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub config: AppConfig,
    pub event_hub: SharedImEventHub,
}

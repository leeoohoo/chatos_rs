use crate::config::AppConfig;
use crate::db::connect_pool;
use crate::store::AppStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let pool = connect_pool(&config).await?;
        let store = AppStore::new(pool);
        store.ensure_default_super_admin(&config).await?;
        Ok(Self { config, store })
    }
}

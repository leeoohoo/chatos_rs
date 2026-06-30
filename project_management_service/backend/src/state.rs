use crate::config::AppConfig;
use crate::store::AppStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let store = AppStore::new(&config.database_url).await?;
        Ok(Self { config, store })
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::connect_database;
use crate::login_throttle::LoginThrottle;
use crate::store::AppStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
    pub login_throttle: LoginThrottle,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let db = connect_database(&config).await?;
        let store = AppStore::new(db);
        store.initialize().await?;
        store.ensure_default_super_admin(&config).await?;
        Ok(Self {
            config,
            store,
            login_throttle: LoginThrottle::default(),
        })
    }
}

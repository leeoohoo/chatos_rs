// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::connect_database;
use crate::login_throttle::LoginThrottle;
use crate::store::AppStore;
use tracing::info;

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
        let migrated_model_count = store.migrate_legacy_model_task_enabled().await?;
        if migrated_model_count > 0 {
            info!(
                migrated_model_count,
                "migrated legacy model enabled state into task_enabled"
            );
        }
        store.ensure_default_super_admin(&config).await?;
        Ok(Self {
            config,
            store,
            login_throttle: LoginThrottle::default(),
        })
    }

    #[cfg(test)]
    pub(crate) async fn new_without_external_dependencies(
        config: AppConfig,
    ) -> Result<Self, String> {
        let db = connect_database(&config).await?;
        Ok(Self {
            config,
            store: AppStore::new(db),
            login_throttle: LoginThrottle::default(),
        })
    }
}

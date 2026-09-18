// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::connect_database;
use crate::login_throttle::LoginThrottle;
use crate::retention::UserDataRetention;
use crate::store::AppStore;
use crate::wechat::WeChatMiniProgramClient;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
    pub login_throttle: LoginThrottle,
    pub retention: UserDataRetention,
    pub wechat_mini_program: Option<WeChatMiniProgramClient>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let db = connect_database(&config).await?;
        let login_throttle = LoginThrottle::new(db.clone());
        let retention = UserDataRetention::new(
            db.clone(),
            config.retention_interval,
            config.retention_batch_size,
        )?;
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
        let wechat_mini_program = WeChatMiniProgramClient::from_config(&config)?;
        Ok(Self {
            config,
            store,
            login_throttle,
            retention,
            wechat_mini_program,
        })
    }

    #[cfg(test)]
    pub(crate) async fn new_without_external_dependencies(
        config: AppConfig,
    ) -> Result<Self, String> {
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .map_err(|err| format!("create lazy user service PostgreSQL pool failed: {err}"))?;
        let login_throttle = LoginThrottle::new(db.clone());
        let retention = UserDataRetention::new(
            db.clone(),
            config.retention_interval,
            config.retention_batch_size,
        )?;
        let wechat_mini_program = WeChatMiniProgramClient::from_config(&config)?;
        Ok(Self {
            config,
            store: AppStore::new(db),
            login_throttle,
            retention,
            wechat_mini_program,
        })
    }
}

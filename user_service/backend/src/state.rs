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
    pub memory_engine_http_client: Option<reqwest::Client>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let memory_engine_http_client = match (
            config.memory_engine_base_url.as_ref(),
            config.memory_engine_mtls_ca_cert_path.as_ref(),
            config.memory_engine_mtls_client_identity_path.as_ref(),
        ) {
            (Some(_), Some(ca), Some(identity)) => {
                Some(chatos_service_runtime::build_mtls_http_client(
                    chatos_service_runtime::HttpClientTimeouts::new(
                        std::time::Duration::from_millis(
                            config.downstream_request_timeout_ms.max(300) as u64,
                        ),
                    ),
                    ca.as_path(),
                    identity.as_path(),
                )?)
            }
            (None, _, _) => None,
            _ => return Err("Memory Engine mTLS client material is incomplete".to_string()),
        };
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
            memory_engine_http_client,
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
            memory_engine_http_client: None,
        })
    }
}

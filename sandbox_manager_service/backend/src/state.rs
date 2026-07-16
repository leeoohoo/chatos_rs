// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use crate::backend::{build_backend, SandboxBackendRef};
use crate::config::AppConfig;
use crate::pool::SandboxPool;
use crate::service::SandboxManager;
use crate::store::SandboxStore;
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};

#[derive(Clone)]
pub struct AppState {
    pub manager: SandboxManager,
    user_service_http: reqwest::Client,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let store = SandboxStore::new(&config.database_url, &config.mongodb_database).await?;
        let reconciled_slots = store
            .reconcile_active_capacity_slots(config.pool_max_active)
            .await?;
        if reconciled_slots > 0 {
            tracing::info!(
                reconciled_slots,
                "reconciled sandbox active capacity slots on startup"
            );
        }
        let backend: SandboxBackendRef = build_backend(&config);
        let pool = Arc::new(SandboxPool::new(
            config.pool_max_active,
            config.pool_max_pending,
        ));
        let user_service_http = build_http_client(HttpClientTimeouts::new(Duration::from_millis(
            config.user_service_request_timeout_ms.max(300),
        )))
        .map_err(|err| format!("build user_service client failed: {err}"))?;
        let manager = SandboxManager::new(config, store, backend, pool).await?;
        Ok(Self {
            manager,
            user_service_http,
        })
    }

    pub(crate) fn user_service_http(&self) -> &reqwest::Client {
        &self.user_service_http
    }

    pub fn spawn_cleanup_worker(&self) -> tokio::task::JoinHandle<()> {
        self.manager.clone().spawn_cleanup_worker()
    }
}

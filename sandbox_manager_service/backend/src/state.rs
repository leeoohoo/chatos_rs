// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use crate::backend::{build_backend, SandboxBackendRef};
use crate::config::AppConfig;
use crate::pool::SandboxPool;
use crate::service::SandboxManager;
use crate::store::SandboxStore;

#[derive(Clone)]
pub struct AppState {
    pub manager: SandboxManager,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let store = SandboxStore::new(&config.database_url, &config.mongodb_database).await?;
        let backend: SandboxBackendRef = build_backend(&config);
        let pool = Arc::new(SandboxPool::new(
            config.pool_max_active,
            config.pool_max_pending,
        ));
        let manager = SandboxManager::new(config, store, backend, pool).await?;
        Ok(Self { manager })
    }

    pub fn spawn_cleanup_worker(&self) -> tokio::task::JoinHandle<()> {
        self.manager.clone().spawn_cleanup_worker()
    }
}

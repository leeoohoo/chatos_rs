// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::relay::ConnectorRelay;
use crate::store::ConnectorStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub relay: ConnectorRelay,
    pub store: ConnectorStore,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let store = ConnectorStore::connect(&config.database_url).await?;
        Ok(Self {
            config,
            relay: ConnectorRelay::default(),
            store,
        })
    }
}

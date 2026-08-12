// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::OnceLock;

use chatos_cloud_agent_runtime::CloudAgentStateStore;

static CLOUD_AGENT_STORE: OnceLock<CloudAgentStateStore> = OnceLock::new();

pub async fn initialize() -> Result<(), String> {
    let database = crate::db::get_db().await?;
    let (client, database) = database.mongodb_parts();
    let store = CloudAgentStateStore::from_mongodb_database(client, database).await?;
    CLOUD_AGENT_STORE
        .set(store)
        .map_err(|_| "ChatOS Cloud Agent store already initialized".to_string())
}

pub fn store() -> Result<CloudAgentStateStore, String> {
    CLOUD_AGENT_STORE
        .get()
        .cloned()
        .ok_or_else(|| "ChatOS Cloud Agent store is not initialized".to_string())
}

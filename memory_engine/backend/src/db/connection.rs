// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;

pub async fn init_pool(config: &AppConfig) -> Result<chatos_postgres::PgPool, String> {
    let role = match (config.api_enabled, config.worker_enabled) {
        (true, false) => "api",
        (false, true) => "worker",
        _ => "all",
    };
    let config = chatos_postgres::PostgresConfig::from_env(
        config.database_url.clone(),
        format!("memory-engine-{role}"),
        "MEMORY_ENGINE",
    )
        .map_err(|error| error.to_string())?;
    chatos_postgres::connect(&config)
        .await
        .map_err(|error| error.to_string())
}

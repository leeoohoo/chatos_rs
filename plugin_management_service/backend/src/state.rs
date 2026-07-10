// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::Client;
use tracing::warn;

use crate::auth::login_via_user_service;
use crate::config::AppConfig;
use crate::models::LoginRequest;
use crate::seed::seed_system_resources;
use crate::store::AppStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let client = Client::with_uri_str(config.database_url.as_str())
            .await
            .map_err(|err| format!("connect MongoDB failed: {err}"))?;
        let db = client.database(config.mongodb_database.as_str());
        let store = AppStore::new(db);
        store.initialize().await?;
        if config.seed_system_resources {
            let admin_user_id = resolve_seed_admin_user_id(&config).await;
            seed_system_resources(&store, admin_user_id.as_str()).await?;
        }
        Ok(Self { config, store })
    }
}

async fn resolve_seed_admin_user_id(config: &AppConfig) -> String {
    match login_via_user_service(
        config,
        &LoginRequest {
            username: config.super_admin_username.clone(),
            password: config.super_admin_password.clone(),
        },
    )
    .await
    {
        Ok(response) => response.user.user_id,
        Err(err) => {
            warn!(
                error = err.as_str(),
                "failed to resolve plugin management seed admin user from user_service"
            );
            fallback_admin_user_id(config.super_admin_username.as_str())
        }
    }
}

fn fallback_admin_user_id(username: &str) -> String {
    let normalized = username.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        "admin".to_string()
    } else {
        format!("admin:{normalized}")
    }
}

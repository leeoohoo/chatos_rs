// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::Client;
use tracing::warn;

use chatos_service_runtime::{build_http_client, HttpClientTimeouts};

use crate::auth::login_via_user_service;
use crate::config::AppConfig;
use crate::models::LoginRequest;
use crate::seed::{ensure_agent_prompt_version_history, seed_system_resources};
use crate::store::AppStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
    pub(crate) user_service_http: reqwest::Client,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let client = Client::with_uri_str(config.database_url.as_str())
            .await
            .map_err(|err| format!("connect MongoDB failed: {err}"))?;
        let db = client.database(config.mongodb_database.as_str());
        let store = AppStore::new(db);
        store.initialize().await?;
        let user_service_http =
            build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
                .map_err(|err| format!("build user_service client failed: {err}"))?;
        if config.seed_system_resources {
            let admin_user_id = resolve_seed_admin_user_id(&config, &user_service_http).await;
            seed_system_resources(&store, admin_user_id.as_str()).await?;
        }
        ensure_agent_prompt_version_history(&store).await?;
        Ok(Self {
            config,
            store,
            user_service_http,
        })
    }

    pub(crate) fn user_service_http(&self) -> &reqwest::Client {
        &self.user_service_http
    }
}

async fn resolve_seed_admin_user_id(config: &AppConfig, client: &reqwest::Client) -> String {
    match login_via_user_service(
        config,
        client,
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

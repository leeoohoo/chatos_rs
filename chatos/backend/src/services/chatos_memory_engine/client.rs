// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use memory_engine_sdk::MemoryEngineClient;

use crate::config::Config;
use crate::services::access_token_scope;

use super::CHATOS_COMPAT_SOURCE_ID;

fn build_client_from_config() -> Result<MemoryEngineClient, String> {
    let cfg = Config::try_get()?;
    let client = MemoryEngineClient::new_direct_with_http_client(
        cfg.memory_engine_base_url.clone(),
        CHATOS_COMPAT_SOURCE_ID.to_string(),
        cfg.memory_engine_http_client.clone(),
    );
    Ok(apply_data_auth(client, cfg))
}

pub(crate) fn apply_data_auth(mut client: MemoryEngineClient, cfg: &Config) -> MemoryEngineClient {
    let access_token = access_token_scope::get_current_access_token();
    let internal_secret = cfg.memory_engine_operator_token.as_deref();
    if access_token_scope::prefer_internal_memory_service_auth() {
        if let Some(internal_secret) = internal_secret {
            return client.with_internal_service_auth("chatos-backend", internal_secret);
        }
    }
    if let Some(access_token) = access_token {
        client = client.with_bearer_token(access_token);
    } else if let Some(internal_secret) = internal_secret {
        client = client.with_internal_service_auth("chatos-backend", internal_secret);
    }
    client
}

pub(super) fn build_client() -> Result<MemoryEngineClient, String> {
    build_client_from_config()
}

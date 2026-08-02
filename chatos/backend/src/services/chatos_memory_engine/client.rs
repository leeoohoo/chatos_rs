// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use memory_engine_sdk::MemoryEngineClient;

use crate::config::Config;
use crate::services::access_token_scope;

use super::CHATOS_COMPAT_SOURCE_ID;

fn build_client_with_timeout_ms(timeout_ms: i64) -> Result<MemoryEngineClient, String> {
    let cfg = Config::try_get()?;
    let timeout = Duration::from_millis(timeout_ms.max(300) as u64);
    let client = MemoryEngineClient::new_direct(
        cfg.memory_engine_base_url.clone(),
        timeout,
        CHATOS_COMPAT_SOURCE_ID.to_string(),
    )?;
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
    let cfg = Config::try_get()?;
    build_client_with_timeout_ms(cfg.memory_engine_request_timeout_ms)
}

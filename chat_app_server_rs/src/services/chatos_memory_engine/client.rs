use std::time::Duration;

use memory_engine_sdk::MemoryEngineClient;

use crate::config::Config;

use super::CHATOS_COMPAT_SOURCE_ID;

fn build_client_with_timeout_ms(timeout_ms: i64) -> Result<MemoryEngineClient, String> {
    let cfg = Config::try_get()?;
    let timeout = Duration::from_millis(timeout_ms.max(300) as u64);
    MemoryEngineClient::new_direct(
        cfg.memory_engine_base_url.clone(),
        timeout,
        CHATOS_COMPAT_SOURCE_ID.to_string(),
    )
}

pub(super) fn build_client() -> Result<MemoryEngineClient, String> {
    let cfg = Config::try_get()?;
    build_client_with_timeout_ms(cfg.memory_engine_request_timeout_ms)
}

pub(super) fn build_active_summary_trigger_client() -> Result<MemoryEngineClient, String> {
    let cfg = Config::try_get()?;
    build_client_with_timeout_ms(cfg.memory_engine_active_summary_trigger_timeout_ms)
}

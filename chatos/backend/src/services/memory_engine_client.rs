// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::Config;

pub type EngineSourceDto = memory_engine_sdk::EngineSource;
pub type UpsertEngineSourceRequestDto = memory_engine_sdk::UpsertSourceRequest;

fn build_client() -> Result<memory_engine_sdk::MemoryEngineClient, String> {
    let cfg = Config::try_get()?;
    let mut client = memory_engine_sdk::MemoryEngineClient::new_platform(
        cfg.memory_engine_base_url.clone(),
        std::time::Duration::from_millis(cfg.memory_engine_request_timeout_ms.max(300) as u64),
    )?;
    if let Some(operator_token) = cfg.memory_engine_operator_token.as_deref() {
        client = client.with_internal_service_auth("chatos-backend", operator_token);
    }
    Ok(client)
}

pub async fn upsert_source(
    source_id: &str,
    req: &UpsertEngineSourceRequestDto,
) -> Result<EngineSourceDto, String> {
    build_client()?.upsert_source(source_id, req).await
}

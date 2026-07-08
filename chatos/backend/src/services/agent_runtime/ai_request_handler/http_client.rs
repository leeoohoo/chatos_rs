// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use tracing::warn;

use super::AGENT_RUNTIME_LOG_PREFIX;

const UPSTREAM_CONNECT_TIMEOUT_MS_ENV: &str = "AI_AGENT_UPSTREAM_CONNECT_TIMEOUT_MS";
const UPSTREAM_READ_TIMEOUT_MS_ENV: &str = "AI_AGENT_UPSTREAM_READ_TIMEOUT_MS";
const DEFAULT_UPSTREAM_CONNECT_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_UPSTREAM_READ_TIMEOUT_MS: u64 = 120_000;
const MIN_UPSTREAM_TIMEOUT_MS: u64 = 1_000;
const MAX_UPSTREAM_TIMEOUT_MS: u64 = 600_000;

pub(super) fn build_http_client() -> (reqwest::Client, u64, u64) {
    let connect_timeout_ms = read_timeout_env_ms_with_fallback(
        UPSTREAM_CONNECT_TIMEOUT_MS_ENV,
        None,
        DEFAULT_UPSTREAM_CONNECT_TIMEOUT_MS,
    );
    let read_timeout_ms = read_timeout_env_ms_with_fallback(
        UPSTREAM_READ_TIMEOUT_MS_ENV,
        None,
        DEFAULT_UPSTREAM_READ_TIMEOUT_MS,
    );

    match reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(connect_timeout_ms))
        .read_timeout(Duration::from_millis(read_timeout_ms))
        .build()
    {
        Ok(client) => (client, connect_timeout_ms, read_timeout_ms),
        Err(err) => {
            warn!(
                "{} failed to build reqwest client with timeout config; fallback default client: {}",
                AGENT_RUNTIME_LOG_PREFIX, err
            );
            (reqwest::Client::new(), connect_timeout_ms, read_timeout_ms)
        }
    }
}

#[cfg(test)]
pub(super) fn read_timeout_env_ms(key: &str, default_ms: u64) -> u64 {
    read_timeout_env_ms_with_fallback(key, None, default_ms)
}

fn read_timeout_env_ms_with_fallback(key: &str, legacy_key: Option<&str>, default_ms: u64) -> u64 {
    let parsed = std::env::var(key)
        .ok()
        .or_else(|| legacy_key.and_then(|legacy| std::env::var(legacy).ok()))
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(default_ms);
    parsed.clamp(MIN_UPSTREAM_TIMEOUT_MS, MAX_UPSTREAM_TIMEOUT_MS)
}

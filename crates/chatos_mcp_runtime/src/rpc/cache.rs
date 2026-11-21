// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::types::McpStdioServer;

const MCP_TOOLS_LIST_SUCCESS_CACHE_TTL: Duration = Duration::from_secs(60);
const MCP_TOOLS_LIST_ERROR_CACHE_TTL: Duration = Duration::from_secs(10);
pub(super) const MCP_TOOLS_LIST_CACHE_MAX_ENTRIES: usize = 256;
pub(super) static MCP_TOOLS_LIST_CACHE: OnceLock<Mutex<HashMap<String, ToolsListCacheEntry>>> =
    OnceLock::new();

#[derive(Clone)]
pub(super) struct ToolsListCacheEntry {
    pub(super) expires_at: Instant,
    pub(super) result: Result<Vec<Value>, String>,
}

pub(super) fn cached_tools_list(cache_key: &str) -> Option<Result<Vec<Value>, String>> {
    let cache = MCP_TOOLS_LIST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().ok()?;
    let entry = guard.get(cache_key)?;
    if Instant::now() < entry.expires_at {
        return Some(entry.result.clone());
    }
    guard.remove(cache_key);
    None
}

pub(super) fn store_tools_list_cache(cache_key: String, result: Result<Vec<Value>, String>) {
    let ttl = if result.is_ok() {
        MCP_TOOLS_LIST_SUCCESS_CACHE_TTL
    } else {
        MCP_TOOLS_LIST_ERROR_CACHE_TTL
    };
    let Some(expires_at) = Instant::now().checked_add(ttl) else {
        return;
    };
    let cache = MCP_TOOLS_LIST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        prune_tools_list_cache(&mut guard, Instant::now(), cache_key.as_str());
        guard.insert(cache_key, ToolsListCacheEntry { expires_at, result });
    }
}

pub(super) fn prune_tools_list_cache(
    cache: &mut HashMap<String, ToolsListCacheEntry>,
    now: Instant,
    incoming_key: &str,
) {
    cache.retain(|_, entry| entry.expires_at > now);
    if cache.len() < MCP_TOOLS_LIST_CACHE_MAX_ENTRIES || cache.contains_key(incoming_key) {
        return;
    }
    if let Some(key) = cache
        .iter()
        .min_by_key(|(_, entry)| entry.expires_at)
        .map(|(key, _)| key.clone())
    {
        cache.remove(&key);
    }
}

pub(super) fn tools_list_http_cache_key(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    timeout: Option<Duration>,
) -> String {
    let mut parts = vec![format!("http:url={}", url.trim())];
    if let Some(timeout) = timeout {
        parts.push(format!("timeout_ms={}", timeout.as_millis()));
    }
    if let Some(headers) = headers {
        let mut entries = headers.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (key, value) in entries {
            parts.push(format!(
                "header:{}:sha256={}",
                key.trim(),
                hex::encode(Sha256::digest(value.as_bytes()))
            ));
        }
    }
    parts.join("\n")
}

pub(super) fn tools_list_stdio_cache_key(cfg: &McpStdioServer) -> String {
    let mut parts = vec![format!("stdio:command={}", cfg.command.trim())];
    if let Some(user_id) = &cfg.user_id {
        parts.push(format!("user_id={}", user_id.trim()));
    }
    if let Some(args) = &cfg.args {
        for arg in args {
            parts.push(format!("arg={arg}"));
        }
    }
    if let Some(cwd) = &cfg.cwd {
        parts.push(format!("cwd={}", cwd.trim()));
    }
    if let Some(env) = &cfg.env {
        let mut entries = env.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (key, value) in entries {
            parts.push(format!(
                "env:{}:sha256={}",
                key.trim(),
                hex::encode(Sha256::digest(value.as_bytes()))
            ));
        }
    }
    parts.join("\n")
}

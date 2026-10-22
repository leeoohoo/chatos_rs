// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::ClientStorageError;
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct LocalMemoryContextCacheEntry {
    pub response: Value,
    pub refreshed_at_unix_ms: i64,
    pub cached_message_high_water: u64,
    pub current_message_high_water: u64,
}

#[async_trait]
pub trait LocalMemoryContextCacheStore: Send + Sync {
    async fn put_memory_context_cache(
        &self,
        cache_key: &str,
        tenant_id: &str,
        source_id: &str,
        thread_id: &str,
        response: &Value,
        refreshed_at_unix_ms: i64,
    ) -> Result<(), ClientStorageError>;

    async fn get_memory_context_cache(
        &self,
        cache_key: &str,
    ) -> Result<Option<LocalMemoryContextCacheEntry>, ClientStorageError>;
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::dto::{ResolvedAgentCapabilities, SystemAgentKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolveAuthMode {
    User,
    Internal,
    Proxy,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityCacheKey {
    pub agent_key: SystemAgentKey,
    pub owner_user_id: String,
    pub auth_mode: ResolveAuthMode,
}

#[derive(Clone)]
pub struct CapabilityCache {
    ttl: Duration,
    values: Arc<RwLock<HashMap<CapabilityCacheKey, CacheEntry>>>,
}

#[derive(Clone)]
struct CacheEntry {
    inserted_at: Instant,
    value: ResolvedAgentCapabilities,
}

impl CapabilityCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            values: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, key: &CapabilityCacheKey) -> Option<ResolvedAgentCapabilities> {
        let entry = self.values.read().await.get(key).cloned()?;
        if entry.inserted_at.elapsed() <= self.ttl {
            return Some(entry.value);
        }
        self.values.write().await.remove(key);
        None
    }

    pub async fn insert(&self, key: CapabilityCacheKey, value: ResolvedAgentCapabilities) {
        self.values.write().await.insert(
            key,
            CacheEntry {
                inserted_at: Instant::now(),
                value,
            },
        );
    }

    pub async fn invalidate_owner(&self, owner_user_id: &str) {
        self.values
            .write()
            .await
            .retain(|key, _| key.owner_user_id != owner_user_id);
    }
}

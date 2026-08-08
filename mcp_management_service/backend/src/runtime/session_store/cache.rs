// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use super::{
    PersistedRuntimeSessionSnapshot, RuntimeSessionSnapshot, MAX_PERSISTED_SNAPSHOT_BYTES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeSessionCacheLimits {
    pub max_entries: usize,
    pub max_bytes: usize,
}

impl RuntimeSessionCacheLimits {
    pub fn new(max_entries: usize, max_bytes: usize) -> Result<Self, String> {
        if max_entries == 0 {
            return Err("Runtime Session cache max entries must be at least 1".to_string());
        }
        if max_bytes == 0 {
            return Err("Runtime Session cache max bytes must be at least 1".to_string());
        }
        Ok(Self {
            max_entries,
            max_bytes,
        })
    }
}

#[derive(Clone)]
pub(super) struct CachedRuntimeSessionSnapshot {
    pub(super) envelope_digest: [u8; 32],
    pub(super) snapshot: Arc<RuntimeSessionSnapshot>,
    pub(super) approx_size_bytes: usize,
    last_access_tick: u64,
}

#[derive(Default)]
pub(super) struct RuntimeSessionCache {
    pub(super) entries: HashMap<String, CachedRuntimeSessionSnapshot>,
    pub(super) total_bytes: usize,
    next_access_tick: u64,
    pub(super) hits_total: u64,
    pub(super) misses_total: u64,
    pub(super) capacity_evictions_total: u64,
    pub(super) expired_evictions_total: u64,
    pub(super) oversized_rejections_total: u64,
}

pub(super) fn cache_snapshot(
    cache: &mut RuntimeSessionCache,
    envelope_digest: [u8; 32],
    snapshot: RuntimeSessionSnapshot,
    limits: RuntimeSessionCacheLimits,
) {
    cache_snapshot_arc(cache, envelope_digest, Arc::new(snapshot), limits);
}

pub(super) fn cache_snapshot_arc(
    cache: &mut RuntimeSessionCache,
    envelope_digest: [u8; 32],
    snapshot: Arc<RuntimeSessionSnapshot>,
    limits: RuntimeSessionCacheLimits,
) {
    cache_snapshot_with_raw_limits(
        cache,
        envelope_digest,
        snapshot,
        limits.max_entries,
        limits.max_bytes,
    );
}

#[cfg(test)]
pub(super) fn cache_snapshot_with_limits(
    cache: &mut RuntimeSessionCache,
    envelope_digest: [u8; 32],
    snapshot: Arc<RuntimeSessionSnapshot>,
    max_entries: usize,
    max_bytes: usize,
) {
    cache_snapshot_with_raw_limits(cache, envelope_digest, snapshot, max_entries, max_bytes);
}

fn cache_snapshot_with_raw_limits(
    cache: &mut RuntimeSessionCache,
    envelope_digest: [u8; 32],
    snapshot: Arc<RuntimeSessionSnapshot>,
    max_entries: usize,
    max_bytes: usize,
) {
    let now = chrono::Utc::now().timestamp();
    cache.retain_unexpired(now);
    let approx_size_bytes = estimate_snapshot_cache_bytes(snapshot.as_ref());
    let session_id = snapshot.session_id.clone();
    cache.remove(session_id.as_str());
    if approx_size_bytes > max_bytes {
        cache.oversized_rejections_total = cache.oversized_rejections_total.saturating_add(1);
        return;
    }
    let last_access_tick = cache.allocate_access_tick();
    cache.entries.insert(
        session_id,
        CachedRuntimeSessionSnapshot {
            envelope_digest,
            snapshot,
            approx_size_bytes,
            last_access_tick,
        },
    );
    cache.total_bytes = cache.total_bytes.saturating_add(approx_size_bytes);
    cache.evict_to_limits(max_entries, max_bytes);
}

pub(super) fn estimate_snapshot_cache_bytes(snapshot: &RuntimeSessionSnapshot) -> usize {
    PersistedRuntimeSessionSnapshot::try_from(snapshot)
        .ok()
        .and_then(|persisted| serde_json::to_vec(&persisted).ok().map(|value| value.len()))
        .unwrap_or(MAX_PERSISTED_SNAPSHOT_BYTES)
}

#[derive(Default)]
pub(super) struct SnapshotSizeStats {
    pub(super) total_bytes: usize,
    pub(super) avg_bytes: usize,
    pub(super) p95_bytes: usize,
}

pub(super) fn summarize_snapshot_sizes(snapshot_sizes: &[usize]) -> SnapshotSizeStats {
    if snapshot_sizes.is_empty() {
        return SnapshotSizeStats::default();
    }
    let total_bytes = snapshot_sizes
        .iter()
        .copied()
        .fold(0_usize, usize::saturating_add);
    let avg_bytes = total_bytes / snapshot_sizes.len();
    let mut sorted = snapshot_sizes.to_vec();
    sorted.sort_unstable();
    let p95_index = ((sorted.len() * 95).saturating_sub(1)) / 100;
    SnapshotSizeStats {
        total_bytes,
        avg_bytes,
        p95_bytes: sorted[p95_index],
    }
}

pub(super) fn saturating_u64_to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

impl RuntimeSessionCache {
    fn allocate_access_tick(&mut self) -> u64 {
        self.next_access_tick = self.next_access_tick.saturating_add(1);
        self.next_access_tick
    }

    pub(super) fn get_if_fresh(
        &mut self,
        session_id: &str,
        envelope_digest: [u8; 32],
        now: i64,
    ) -> Option<Arc<RuntimeSessionSnapshot>> {
        self.retain_unexpired(now);
        let Some(cached) = self.entries.get(session_id) else {
            self.misses_total = self.misses_total.saturating_add(1);
            return None;
        };
        if cached.envelope_digest != envelope_digest {
            self.remove(session_id);
            self.misses_total = self.misses_total.saturating_add(1);
            return None;
        }
        let access_tick = self.allocate_access_tick();
        let cached = self.entries.get_mut(session_id)?;
        cached.last_access_tick = access_tick;
        self.hits_total = self.hits_total.saturating_add(1);
        Some(Arc::clone(&cached.snapshot))
    }

    pub(super) fn remove(&mut self, session_id: &str) -> Option<CachedRuntimeSessionSnapshot> {
        let removed = self.entries.remove(session_id)?;
        self.total_bytes = self.total_bytes.saturating_sub(removed.approx_size_bytes);
        Some(removed)
    }

    pub(super) fn retain_unexpired(&mut self, now: i64) {
        let expired = self
            .entries
            .iter()
            .filter(|(_, cached)| cached.snapshot.expires_at_unix <= now)
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        for session_id in expired {
            if self.remove(session_id.as_str()).is_some() {
                self.expired_evictions_total = self.expired_evictions_total.saturating_add(1);
            }
        }
    }

    fn evict_to_limits(&mut self, max_entries: usize, max_bytes: usize) {
        while self.entries.len() > max_entries || self.total_bytes > max_bytes {
            let Some(session_id) = self
                .entries
                .iter()
                .min_by_key(|(_, cached)| {
                    (cached.last_access_tick, cached.snapshot.expires_at_unix)
                })
                .map(|(session_id, _)| session_id.clone())
            else {
                break;
            };
            if self.remove(session_id.as_str()).is_some() {
                self.capacity_evictions_total = self.capacity_evictions_total.saturating_add(1);
            }
        }
    }
}

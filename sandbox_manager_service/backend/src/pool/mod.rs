// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct SandboxPool {
    max_active: AtomicUsize,
    max_pending: AtomicUsize,
    active: AtomicUsize,
    pending: AtomicUsize,
}

pub type SandboxPoolRef = Arc<SandboxPool>;

impl SandboxPool {
    pub fn new(max_active: usize, max_pending: usize) -> Self {
        Self {
            max_active: AtomicUsize::new(max_active.max(1)),
            max_pending: AtomicUsize::new(max_pending),
            active: AtomicUsize::new(0),
            pending: AtomicUsize::new(0),
        }
    }

    pub fn try_acquire_active(&self) -> Result<PoolSlot<'_>, String> {
        loop {
            let current = self.active.load(Ordering::SeqCst);
            let max_active = self.max_active();
            if current >= max_active {
                return Err(format!(
                    "sandbox pool is full: active={current}, max_active={max_active}"
                ));
            }
            if self
                .active
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(PoolSlot {
                    pool: self,
                    released: false,
                });
            }
        }
    }

    pub fn release_active(&self) {
        let _ = self
            .active
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                Some(value.saturating_sub(1))
            });
    }

    pub fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }

    pub fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }

    pub fn max_pending(&self) -> usize {
        self.max_pending.load(Ordering::SeqCst)
    }

    pub fn set_limits(&self, max_active: usize, max_pending: usize) {
        self.max_active.store(max_active.max(1), Ordering::SeqCst);
        self.max_pending.store(max_pending, Ordering::SeqCst);
    }
}

pub struct PoolSlot<'a> {
    pool: &'a SandboxPool,
    released: bool,
}

impl PoolSlot<'_> {
    pub fn commit(mut self) {
        self.released = true;
    }
}

impl Drop for PoolSlot<'_> {
    fn drop(&mut self) {
        if !self.released {
            self.pool.release_active();
        }
    }
}

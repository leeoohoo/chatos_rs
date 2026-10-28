// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Process-local activity count whose meaning is chosen by the owning coordinator field.
///
/// Separate instances track claimed model/tool work, scheduler attempts that passed admission,
/// and Memory sync. RAII keeps every error and cancellation path balanced.
#[derive(Clone, Default)]
pub(crate) struct HostWorkActivity {
    active: Arc<AtomicUsize>,
}

impl HostWorkActivity {
    pub(crate) fn begin(&self) -> HostWorkActivityGuard {
        self.active.fetch_add(1, Ordering::AcqRel);
        HostWorkActivityGuard {
            active: Arc::clone(&self.active),
        }
    }

    pub(crate) fn active_count(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
}

pub(crate) struct HostWorkActivityGuard {
    active: Arc<AtomicUsize>,
}

impl Drop for HostWorkActivityGuard {
    fn drop(&mut self) {
        let previous = self.active.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "Host work activity counter underflow");
    }
}

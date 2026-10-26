// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Process-local count of Host workers that already own a durable model or tool claim.
///
/// The counter is shared by the model and tool schedulers. A guard is created only after a
/// claim succeeds and is released on every completion, cancellation, lost-lease, and error path.
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

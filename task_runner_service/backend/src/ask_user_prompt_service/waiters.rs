// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AskUserPromptWaiters {
    pub(super) fn register(&self, prompt_id: &str) -> Arc<Notify> {
        let mut inner = self.inner.lock();
        let notify = Arc::new(Notify::new());
        inner.insert(prompt_id.to_string(), notify.clone());
        notify
    }

    pub(super) fn wake(&self, prompt_id: &str) {
        if let Some(notify) = self.inner.lock().get(prompt_id).cloned() {
            notify.notify_waiters();
        }
    }

    pub(super) fn remove(&self, prompt_id: &str) {
        self.inner.lock().remove(prompt_id);
    }
}

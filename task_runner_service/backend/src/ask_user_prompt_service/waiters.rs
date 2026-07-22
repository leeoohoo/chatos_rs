// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) struct AskUserPromptWaiterRegistration {
    waiters: AskUserPromptWaiters,
    prompt_id: String,
    notify: Arc<Notify>,
}

impl AskUserPromptWaiterRegistration {
    pub(super) fn notify(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }
}

impl Drop for AskUserPromptWaiterRegistration {
    fn drop(&mut self) {
        self.waiters.remove(self.prompt_id.as_str());
    }
}

impl AskUserPromptWaiters {
    pub(super) fn register(&self, prompt_id: &str) -> AskUserPromptWaiterRegistration {
        let mut inner = self.inner.lock();
        let notify = Arc::new(Notify::new());
        inner.insert(prompt_id.to_string(), notify.clone());
        AskUserPromptWaiterRegistration {
            waiters: self.clone(),
            prompt_id: prompt_id.to_string(),
            notify,
        }
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

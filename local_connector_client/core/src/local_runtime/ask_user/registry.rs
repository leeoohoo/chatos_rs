// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};

#[derive(Debug, Clone, Default)]
pub(crate) struct LocalAskUserPromptRegistry {
    pending: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl LocalAskUserPromptRegistry {
    pub(crate) async fn register(&self, prompt_id: &str) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        self.pending
            .lock()
            .await
            .insert(prompt_id.to_string(), notify.clone());
        notify
    }

    pub(crate) async fn notify(&self, prompt_id: &str) {
        if let Some(notify) = self.pending.lock().await.get(prompt_id).cloned() {
            notify.notify_waiters();
        }
    }

    pub(crate) async fn remove(&self, prompt_id: &str) {
        self.pending.lock().await.remove(prompt_id);
    }
}

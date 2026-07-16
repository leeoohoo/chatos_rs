// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub(crate) struct LocalEnvironmentJobRegistry {
    active: Arc<Mutex<HashSet<String>>>,
}

impl LocalEnvironmentJobRegistry {
    pub(crate) async fn register(&self, project_id: &str) -> bool {
        self.active.lock().await.insert(project_id.to_string())
    }

    pub(crate) async fn remove(&self, project_id: &str) {
        self.active.lock().await.remove(project_id);
    }
}

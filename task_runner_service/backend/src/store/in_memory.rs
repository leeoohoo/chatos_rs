// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod models;
mod prompts;
mod runs;
mod skills;
mod tasks;
mod users;

impl InMemoryStore {
    pub(crate) fn new(run_event_sender: broadcast::Sender<TaskRunEventRecord>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreData::default())),
            run_event_sender,
        }
    }
}

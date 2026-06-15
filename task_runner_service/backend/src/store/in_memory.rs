use super::*;

mod models;
mod prompts;
mod runs;
mod tasks;
mod users;

impl InMemoryStore {
    pub(super) fn new(run_event_sender: broadcast::Sender<TaskRunEventRecord>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreData::default())),
            run_event_sender,
        }
    }
}

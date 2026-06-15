use super::*;

mod batch_schedule;
mod runtime_settings;
mod tasks;
mod validation;

impl TaskService {
    pub(crate) fn new(config: AppConfig, store: AppStore) -> Self {
        Self { config, store }
    }
}

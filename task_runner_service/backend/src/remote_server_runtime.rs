// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::store::AppStore;

mod connector;
mod ops;
mod store_helpers;
mod support;

pub use self::connector::test_remote_server_connectivity;

#[derive(Clone)]
pub struct TaskRunnerRemoteConnectionStore {
    config: AppConfig,
    store: AppStore,
}

impl TaskRunnerRemoteConnectionStore {
    pub(crate) fn new(config: AppConfig, store: AppStore) -> Self {
        Self { config, store }
    }
}

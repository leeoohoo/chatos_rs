// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::store::AppStore;

mod ops;
mod ssh;
mod store_helpers;
mod support;

pub use self::ssh::test_remote_server_connectivity;

#[derive(Clone)]
pub struct TaskRunnerRemoteConnectionStore {
    store: AppStore,
}

impl TaskRunnerRemoteConnectionStore {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }
}

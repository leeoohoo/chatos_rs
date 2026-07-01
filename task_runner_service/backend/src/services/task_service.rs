// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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

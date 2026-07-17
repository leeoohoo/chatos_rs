// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

pub fn load_task_runner_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

#[cfg(test)]
pub(super) fn task_runner_dotenv_files() -> Vec<PathBuf> {
    chatos_service_runtime::service_dotenv_files(Path::new(env!("CARGO_MANIFEST_DIR")))
}

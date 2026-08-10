// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod aliases;
mod diff;
mod edit;
mod fs_ops;
mod outcome;
mod registration_read;
mod registration_write;
mod revision;
mod service;
mod session;
mod storage;
#[cfg(test)]
mod tests;
mod utils;

pub use self::outcome::{classify_file_modification_error, FileModificationOutcome};
pub use self::service::{
    CodeMaintainerHooks, CodeMaintainerHooksRef, CodeMaintainerOptions, CodeMaintainerService,
};
pub use self::utils::{generate_id, now_iso, resolve_state_dir};

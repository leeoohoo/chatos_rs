// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod aliases;
mod diff;
mod edit;
mod fs_ops;
mod patch;
mod registration_read;
mod registration_write;
mod service;
mod storage;
#[cfg(test)]
mod tests;
mod utils;

pub use self::patch::{apply_patch_limited, ApplyPatchResult};
pub use self::service::{
    CodeMaintainerHooks, CodeMaintainerHooksRef, CodeMaintainerOptions, CodeMaintainerService,
};
pub use self::utils::{generate_id, now_iso, resolve_state_dir};

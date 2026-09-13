// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod approval_context;
mod approval_history_ipc;
mod assembly;
mod attachment_grants;
mod bootstrap;
mod capability_ipc;
mod capability_loader;
mod capability_runtime;
mod capability_validation;
mod client_setting_ipc;
mod clipboard_ipc;
mod context_runtime;
mod host;
mod ipc_server;
mod ipc_transport_common;
#[cfg(unix)]
mod ipc_transport_unix;
#[cfg(windows)]
mod ipc_transport_windows;
mod lifecycle;
mod main_chat_context;
mod media_ipc;
mod memory_sync_worker;
mod notepad_ipc;
mod platform_credentials;
mod platform_storage;
mod plugin_state_ipc;
mod process;
mod profile_context_support;
mod profile_registry;
mod project_ipc;
mod storage_ipc;
mod story_ipc;
mod task_context;
mod task_planner;
mod terminal_history_ipc;
mod tool_runtime;
mod worker;

pub use approval_context::*;
pub use approval_history_ipc::*;
pub use assembly::*;
pub use attachment_grants::*;
pub use bootstrap::*;
pub use capability_ipc::*;
pub use capability_loader::*;
pub use capability_runtime::*;
pub use client_setting_ipc::*;
pub use clipboard_ipc::*;
pub use context_runtime::*;
pub use host::*;
pub use ipc_server::*;
#[cfg(unix)]
pub use ipc_transport_unix::*;
#[cfg(windows)]
pub use ipc_transport_windows::*;
pub use lifecycle::*;
pub use main_chat_context::*;
pub use media_ipc::*;
pub use memory_sync_worker::*;
pub use notepad_ipc::*;
pub use platform_credentials::*;
pub use platform_storage::*;
pub use plugin_state_ipc::*;
pub use process::*;
pub use profile_registry::*;
pub use project_ipc::*;
pub use storage_ipc::*;
pub use story_ipc::*;
pub use task_context::*;
pub use task_planner::*;
pub use terminal_history_ipc::*;
pub use tool_runtime::*;
pub use worker::*;

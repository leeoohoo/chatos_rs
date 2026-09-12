// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod assembly;
mod attachment_grants;
mod bootstrap;
mod capability_loader;
mod capability_runtime;
mod capability_validation;
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
mod platform_credentials;
mod platform_storage;
mod process;
mod profile_registry;
mod storage_ipc;
mod task_context;
mod task_planner;
mod tool_runtime;
mod worker;

pub use assembly::*;
pub use attachment_grants::*;
pub use bootstrap::*;
pub use capability_loader::*;
pub use capability_runtime::*;
pub use context_runtime::*;
pub use host::*;
pub use ipc_server::*;
#[cfg(unix)]
pub use ipc_transport_unix::*;
#[cfg(windows)]
pub use ipc_transport_windows::*;
pub use lifecycle::*;
pub use main_chat_context::*;
pub use platform_credentials::*;
pub use platform_storage::*;
pub use process::*;
pub use profile_registry::*;
pub use storage_ipc::*;
pub use task_context::*;
pub use task_planner::*;
pub use tool_runtime::*;
pub use worker::*;

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod prompt;
pub(crate) mod resolver;
mod selection;
mod sync;

pub(crate) use prompt::merge_system_prompts;
pub(crate) use resolver::resolve_local_chat_capabilities;
pub(crate) use sync::{sync_local_capability_snapshots, sync_local_plugin_control_plane};

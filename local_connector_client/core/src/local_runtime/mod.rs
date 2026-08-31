// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod agent_prompts;
pub(crate) mod api;
mod capabilities;
mod storage;

use chatos_plugin_management_sdk::SystemAgentKey;

pub(crate) const LOCAL_RUNTIME_AGENT_KEYS: [SystemAgentKey; 1] =
    [SystemAgentKey::LocalConnectorCommandApprovalAgent];

pub(crate) use agent_prompts::{
    agent_prompt_status, check_agent_prompt_updates, load_installed_agent_prompt_from_database,
    update_agent_prompt_bundle, LocalAgentPromptStatus,
};
pub(crate) use capabilities::{fetch_all_capability_snapshots, sync_local_plugin_control_plane};
pub(crate) use storage::{database_path_for_state, embedded_migration_versions, LocalDatabase};

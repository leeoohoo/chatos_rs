// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod agent_prompts;
pub(crate) mod api;
mod ask_user;
mod capabilities;
mod chat;
mod environment;
mod memory;
mod memory_policy;
mod model;
mod project_management;
mod storage;
mod task_board;
mod task_runner;

use std::path::{Path, PathBuf};

pub(crate) const LOCAL_UNSCOPED_PROJECT_ID: &str = "-1";
pub(crate) const LOCAL_UNSCOPED_WORKSPACE_ID: &str = "local_runtime_unscoped_workspace";
pub(crate) const LOCAL_UNSCOPED_PROJECT_NAME: &str = "Local Contacts";

pub(crate) fn local_unscoped_workspace_root(state_path: &Path) -> PathBuf {
    state_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("unscoped-workspace")
}

pub(crate) use agent_prompts::{
    agent_prompt_status, check_agent_prompt_updates, load_installed_agent_prompt,
    load_installed_agent_prompt_from_database, spawn_agent_prompt_update_checker,
    update_agent_prompt_bundle, LocalAgentPromptStatus,
};
pub(crate) use ask_user::LocalAskUserPromptRegistry;
pub(crate) use capabilities::{
    fetch_all_capability_snapshots, sync_local_capability_snapshots,
    sync_local_plugin_control_plane,
};
pub(crate) use chat::LocalTurnControlRegistry;
pub(crate) use environment::LocalEnvironmentJobRegistry;
pub(crate) use environment::{
    run_local_environment_analysis, LocalEnvironmentProgressRecord,
    LocalRuntimeEnvironmentImageRecord, LocalRuntimeEnvironmentRecord,
};
pub(crate) use memory::LocalMemoryJobRegistry;
pub(crate) use memory_policy::{managed_memory_policy, sync_managed_memory_policy};
pub(crate) use storage::{database_path_for_state, LocalDatabase};
pub(crate) use task_runner::{run_local_task_worker_loop, EnqueueLocalTaskRunInput};

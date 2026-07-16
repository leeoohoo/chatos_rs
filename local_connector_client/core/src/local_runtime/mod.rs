// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) mod api;
mod ask_user;
mod capabilities;
mod chat;
mod environment;
mod memory;
mod model;
mod project_management;
mod storage;
mod task_board;
mod task_runner;

pub(crate) use ask_user::LocalAskUserPromptRegistry;
pub(crate) use capabilities::{sync_local_capability_snapshots, sync_local_plugin_control_plane};
pub(crate) use chat::LocalTurnControlRegistry;
pub(crate) use environment::LocalEnvironmentJobRegistry;
pub(crate) use environment::{
    run_local_environment_analysis, LocalEnvironmentProgressRecord,
    LocalRuntimeEnvironmentImageRecord, LocalRuntimeEnvironmentRecord,
};
pub(crate) use memory::LocalMemoryJobRegistry;
pub(crate) use storage::{database_path_for_state, LocalDatabase};
pub(crate) use task_runner::{run_local_task_worker_loop, EnqueueLocalTaskRunInput};

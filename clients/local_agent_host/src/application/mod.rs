// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Application orchestration for profiles, tasks, models, and tools.

mod assembly;
mod coordinator;
mod memory_sync;
mod scheduler;
mod task_tools;
mod tool_scheduler;

pub use assembly::LocalAgentHostAssembly;
pub use chatos_local_agent_runtime::LocalAgentRuntime;
pub use coordinator::{LocalAgentCoordinatorError, LocalAgentHostCoordinator};
pub use memory_sync::{
    LocalMemoryContextCache, LocalMemoryOutboxWriter, LocalMemorySyncError, LocalMemorySyncWorker,
    MemorySyncTick,
};
pub use scheduler::{LocalAgentScheduler, LocalAgentSchedulerError, SchedulerTick};
pub use task_tools::{LocalTaskToolExecutor, CREATE_TASKS_TOOL, CREATE_TASK_TOOL};
pub use tool_scheduler::{
    LocalToolExecutor, LocalToolRegistry, LocalToolScheduler, LocalToolSchedulerError,
    ToolSchedulerTick,
};

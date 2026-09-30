// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Application orchestration for profiles, tasks, models, and tools.

mod assembly;
mod coordinator;
#[cfg(test)]
mod coordinator_worker_tests;
mod memory_sync;
#[cfg(test)]
mod memory_sync_tests;
#[cfg(test)]
mod notepad_tool_tests;
mod notepad_tools;
mod requirement_survey_tools;
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
pub use notepad_tools::{
    notepad_model_tools, LocalNotepadToolExecutor, NOTEPAD_CREATE_NOTE_TOOL,
    NOTEPAD_LIST_FOLDERS_TOOL, NOTEPAD_LIST_NOTES_TOOL, NOTEPAD_READ_NOTE_TOOL,
    NOTEPAD_READ_ONLY_TOOLS, NOTEPAD_TOOL_NAMES, NOTEPAD_UPDATE_NOTE_TOOL,
};
pub use requirement_survey_tools::{
    requirement_survey_model_tools, LocalRequirementSurveyToolExecutor,
    REQUIREMENT_SURVEY_CREATE_TOOL, REQUIREMENT_SURVEY_TOOL_NAMES,
};
pub use scheduler::{LocalAgentScheduler, LocalAgentSchedulerError, SchedulerTick};
pub use task_tools::{
    task_model_tools, LocalTaskToolExecutor, CREATE_TASKS_TOOL, CREATE_TASK_TOOL,
};
pub use tool_scheduler::{
    LocalToolExecutor, LocalToolRegistry, LocalToolScheduler, LocalToolSchedulerError,
    ToolSchedulerTick,
};

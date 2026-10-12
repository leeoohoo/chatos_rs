// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Application orchestration for profiles, tasks, models, and tools.

#[cfg(test)]
mod ask_user_tool_tests;
mod ask_user_tools;
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
mod task_model_policy;
mod task_process_tools;
mod task_tool_definitions;
mod task_tool_executor_support;
#[cfg(test)]
mod task_tool_scope_tests;
mod task_tool_source_context;
mod task_tool_support;
mod task_tools;
mod tool_scheduler;

pub use ask_user_tools::{
    ask_user_model_tools, LocalAskUserToolExecutor, ASK_USER_CHOICES_TOOL,
    ASK_USER_KEY_VALUES_TOOL, ASK_USER_MIXED_FORM_TOOL, ASK_USER_TOOL_NAMES,
};
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
pub use task_process_tools::{
    task_process_model_tools, task_process_prompt_item, LocalTaskProcessToolExecutor,
    TASK_OUTCOME_REPORT_TOOL, TASK_PROCESS_RECORD_TOOL, TASK_PROCESS_TOOL_NAMES,
};
pub use task_tool_definitions::{
    task_model_tools, CANCEL_TASK_TOOL, CREATE_TASKS_TOOL, CREATE_TASK_TOOL,
    GET_TASK_DEPENDENCY_GRAPH_TOOL, GET_TASK_TOOL, LIST_TASKS_TOOL, TASK_APPROVAL_EXEMPT_TOOLS,
    TASK_READ_ONLY_TOOLS, TASK_TOOL_NAMES, WAIT_FOR_TASK_COMPLETION_TOOL,
};
pub use task_tools::LocalTaskToolExecutor;
pub use tool_scheduler::{
    LocalToolExecutor, LocalToolRegistry, LocalToolScheduler, LocalToolSchedulerError,
    ToolSchedulerTick,
};

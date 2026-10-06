// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Client-owned Local Agent Host composition root.

pub mod application;
pub mod infrastructure;
pub mod interface;

pub use application::{
    LocalAgentCoordinatorError, LocalAgentHostAssembly, LocalAgentHostCoordinator,
    LocalAgentScheduler, LocalAgentSchedulerError, LocalAskUserToolExecutor,
    LocalMemoryContextCache, LocalMemoryOutboxWriter, LocalMemorySyncError, LocalMemorySyncWorker,
    LocalNotepadToolExecutor, LocalRequirementSurveyToolExecutor, LocalTaskProcessToolExecutor,
    LocalTaskToolExecutor, LocalToolExecutor, LocalToolRegistry, LocalToolScheduler,
    LocalToolSchedulerError, MemorySyncTick, SchedulerTick, ToolSchedulerTick,
    ASK_USER_CHOICES_TOOL, ASK_USER_KEY_VALUES_TOOL, ASK_USER_MIXED_FORM_TOOL, ASK_USER_TOOL_NAMES,
    CANCEL_TASK_TOOL, CREATE_TASKS_TOOL, CREATE_TASK_TOOL, GET_TASK_DEPENDENCY_GRAPH_TOOL,
    GET_TASK_TOOL, LIST_TASKS_TOOL, NOTEPAD_CREATE_NOTE_TOOL, NOTEPAD_LIST_FOLDERS_TOOL,
    NOTEPAD_LIST_NOTES_TOOL, NOTEPAD_READ_NOTE_TOOL, NOTEPAD_UPDATE_NOTE_TOOL,
    REQUIREMENT_SURVEY_CREATE_TOOL, TASK_APPROVAL_EXEMPT_TOOLS, TASK_OUTCOME_REPORT_TOOL,
    TASK_PROCESS_RECORD_TOOL, TASK_PROCESS_TOOL_NAMES, TASK_READ_ONLY_TOOLS, TASK_TOOL_NAMES,
    WAIT_FOR_TASK_COMPLETION_TOOL,
};
pub use chatos_agent_profiles::{
    ChatosAiRuntimeStepExecutor, ConservativeToolSafetyPolicy, ControlPlaneLocalAiStepPlanner,
    DurableAiProfile, LocalAiStepExecutor, LocalAiStepPlanner, LocalCapabilityResolver,
    LocalModelRuntimeResolver, NamedReadOnlyTools, PreparedLocalAiStep, ResolvedLocalCapabilities,
    ToolSafetyPolicy, TransientLocalModelRuntime, MAIN_CHAT_PROFILE_KEY,
    TASK_EXECUTION_PROFILE_KEY,
};
pub use infrastructure::{
    ChildEnvironmentModelCredentialResolver, LocalCapabilityPolicySnapshot,
    LocalCapabilitySnapshotStore, LocalControlPlaneSnapshot, LocalJsonSchemaOutputFormat,
    LocalMcpServerConfig, LocalMcpStdioSession, LocalMcpToolDefinition, LocalMcpToolSet,
    LocalMemoryRuntimeConfig, LocalMemoryRuntimeServices, LocalModelConfigSnapshot,
    LocalModelConfigSnapshotStore, LocalModelCredentialResolver, LocalPluginSecretResolver,
};
pub use interface::{
    decode_response, read_frame, serve_reader_writer, serve_stream, write_frame,
    HostRequestHandler, HostTransportError,
};

#[cfg(unix)]
pub use interface::unix;
#[cfg(windows)]
pub use interface::windows;

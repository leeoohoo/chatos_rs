// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Client-owned Local Agent Host composition root.

pub mod application;
pub mod infrastructure;
pub mod interface;

pub use application::{
    LocalAgentCoordinatorError, LocalAgentHostAssembly, LocalAgentHostCoordinator,
    LocalAgentScheduler, LocalAgentSchedulerError, LocalTaskToolExecutor, LocalToolExecutor,
    LocalToolRegistry, LocalToolScheduler, LocalToolSchedulerError, SchedulerTick,
    ToolSchedulerTick, CREATE_TASKS_TOOL, CREATE_TASK_TOOL,
};
pub use chatos_agent_profiles::{
    ChatosAiRuntimeStepExecutor, ConservativeToolSafetyPolicy, ControlPlaneLocalAiStepPlanner,
    DurableAiProfile, LocalAiStepExecutor, LocalAiStepPlanner, LocalCapabilityResolver,
    LocalModelRuntimeResolver, NamedReadOnlyTools, PreparedLocalAiStep, ResolvedLocalCapabilities,
    ToolSafetyPolicy, TransientLocalModelRuntime, MAIN_CHAT_PROFILE_KEY, TASK_RUNNER_PROFILE_KEY,
};
pub use infrastructure::LocalControlPlaneSnapshot;
pub use interface::{
    decode_response, read_frame, serve_reader_writer, serve_stream, write_frame,
    HostRequestHandler, HostTransportError,
};

#[cfg(unix)]
pub use interface::unix;
#[cfg(windows)]
pub use interface::windows;

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
#[cfg(feature = "runtime")]
mod core;
#[cfg(feature = "runtime")]
mod implementations;

pub use catalog::{agent_descriptor, system_agent_catalog, AgentDescriptor};
#[cfg(feature = "runtime")]
pub use core::{
    merge_system_instructions, AgentError, AgentExecutor, AgentIdentity, AgentTurnMemory,
    AgentTurnRequest, SystemAgentDefinition,
};
#[cfg(feature = "runtime")]
pub use implementations::{
    ChatosAgentProfile, ChatosStreamAgent, ChatosStreamRuntime, CommandApprovalAgent,
    ProjectEnvironmentAgent, TaskRunnerAgent, TaskRunnerRunSpecInput, COMMAND_APPROVAL_AGENT,
    PROJECT_ENVIRONMENT_AGENT, TASK_RUNNER_AGENT,
};

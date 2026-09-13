// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
#[cfg(feature = "runtime")]
mod core;
#[cfg(feature = "runtime")]
mod implementations;

pub use catalog::{
    agent_descriptor, can_use_chatos_notepad, is_chatos_conversation_agent,
    is_task_runner_execution_agent, is_task_runner_phase_agent, parse_system_agent_key,
    system_agent_catalog, AgentDescriptor, AgentExecutionLocation,
};
pub use chatos_plugin_management_sdk::SystemAgentKey;
#[cfg(feature = "runtime")]
pub use core::{
    merge_system_instructions, resolve_managed_prompt_by_key_for_model,
    resolve_managed_prompt_by_key_for_model_with_profile, resolve_managed_prompt_for_model,
    resolve_managed_prompt_for_model_with_client, AgentError, AgentIdentity, SystemAgentDefinition,
};
#[cfg(feature = "local-agent-loop")]
pub use core::{AgentExecutor, AgentTurnMemory, AgentTurnRequest};
#[cfg(feature = "runtime")]
pub use implementations::{
    ChatosAgentProfile, ChatosStreamAgent, ChatosStreamRuntime, CommandApprovalAgent,
    TaskRunnerAgent, TaskRunnerRunSpecInput, COMMAND_APPROVAL_AGENT, TASK_RUNNER_AGENT,
};

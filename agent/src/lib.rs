// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
mod config;
#[cfg(feature = "runtime")]
mod core;
#[cfg(feature = "runtime")]
mod implementations;

pub use catalog::{agent_descriptor, system_agent_catalog, AgentDescriptor};
pub use config::{
    agent_max_iterations_from_env, AGENT_MAX_ITERATIONS_CONFIG_KEY, AGENT_MAX_ITERATIONS_ENV,
    DEFAULT_AGENT_MAX_ITERATIONS, LEGACY_CHATOS_MAX_ITERATIONS_ENV,
    LEGACY_TASK_RUNNER_MAX_ITERATIONS_ENV,
};
#[cfg(feature = "managed-config")]
pub use config::{load_agent_max_iterations, resolve_agent_max_iterations};
#[cfg(feature = "runtime")]
pub use core::{
    merge_system_instructions, resolve_managed_prompt_by_key_for_model,
    resolve_managed_prompt_for_model, resolve_managed_prompt_for_model_with_client, AgentError,
    AgentExecutor, AgentIdentity, AgentTurnMemory, AgentTurnRequest, SystemAgentDefinition,
};
#[cfg(feature = "runtime")]
pub use implementations::{
    ChatosAgentProfile, ChatosStreamAgent, ChatosStreamRuntime, CommandApprovalAgent,
    MemoryEngineAgent, MemoryEngineAgentKind, ProjectEnvironmentAgent, TaskRunnerAgent,
    TaskRunnerRunSpecInput, COMMAND_APPROVAL_AGENT, MEMORY_ENGINE_MEMORY_ROLLUP_AGENT,
    MEMORY_ENGINE_ROLLUP_AGENT, MEMORY_ENGINE_SUBJECT_MEMORY_AGENT, MEMORY_ENGINE_SUMMARY_AGENT,
    MEMORY_ENGINE_THREAD_REPAIR_AGENT, PROJECT_ENVIRONMENT_AGENT, TASK_RUNNER_AGENT,
    TASK_RUNNER_PLAN_AGENT,
};

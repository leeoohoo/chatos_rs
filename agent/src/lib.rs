// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;
mod config;
#[cfg(feature = "runtime")]
mod core;
#[cfg(feature = "runtime")]
mod implementations;

pub use catalog::{
    agent_descriptor, chatos_task_runner_tool_profile, is_chatos_callback_agent,
    is_chatos_plan_task_profile, is_project_requirement_execution_planner_agent,
    is_task_runner_execution_agent, is_task_runner_phase_agent, is_task_runner_planning_agent,
    parse_chatos_task_runner_tool_profile, parse_system_agent_key,
    requires_expected_project_task_ids, system_agent_catalog, uses_chatos_browser_callback,
    uses_chatos_notepad_callback, AgentDescriptor, AgentExecutionLocation,
    ChatosTaskRunnerToolProfile, CHATOS_ASYNC_PLANNER_TOOL_PROFILE, CHATOS_PLAN_TASK_PROFILE,
    PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE,
};
pub use chatos_plugin_management_sdk::SystemAgentKey;
#[cfg(feature = "managed-config")]
pub use config::{
    load_agent_max_iterations, require_task_runner_runtime_settings, resolve_agent_max_iterations,
    ManagedRuntimeConfigBundle, RemoteControlTrustConfigBundle,
};
pub use config::{
    TaskRunnerRuntimeSettings, AGENT_MAX_ITERATIONS_CONFIG_KEY, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_TASK_RUNNER_PROMPT_CACHE_ENABLED, DEFAULT_TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED,
    DEFAULT_TASK_RUNNER_REVIEW_MISSING_READ_FAILURES,
    DEFAULT_TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS, DEFAULT_TASK_RUNNER_REVIEW_REPEAT_INTERVAL,
    TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY, TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY,
    TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
    TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
    TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
};
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
    MemoryEngineAgent, MemoryEngineAgentKind, TaskRunnerAgent, TaskRunnerRunSpecInput,
    COMMAND_APPROVAL_AGENT, MEMORY_ENGINE_MEMORY_ROLLUP_AGENT, MEMORY_ENGINE_ROLLUP_AGENT,
    MEMORY_ENGINE_SUBJECT_MEMORY_AGENT, MEMORY_ENGINE_SUMMARY_AGENT,
    MEMORY_ENGINE_THREAD_REPAIR_AGENT, TASK_RUNNER_AGENT, TASK_RUNNER_PLAN_AGENT,
};

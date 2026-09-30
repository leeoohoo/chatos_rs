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
    is_task_runner_execution_agent, is_task_runner_phase_agent,
    parse_chatos_task_runner_tool_profile, parse_system_agent_key, system_agent_catalog,
    uses_chatos_browser_callback, uses_chatos_notepad_callback, AgentDescriptor,
    AgentExecutionLocation, ChatosTaskRunnerToolProfile, CHATOS_ASYNC_PLANNER_TOOL_PROFILE,
};
pub use chatos_plugin_management_sdk::SystemAgentKey;
#[cfg(feature = "managed-config")]
pub use config::{
    load_agent_max_iterations, require_task_runner_runtime_settings, resolve_agent_max_iterations,
    resolve_native_agent_runtime_settings, ManagedRuntimeConfigBundle,
    RemoteControlTrustConfigBundle,
};
pub use config::{
    NativeAgentRuntimeSettings, TaskRunnerRuntimeSettings, AGENT_CONTEXT_WINDOW_TOKENS_CONFIG_KEY,
    AGENT_MAX_ITERATIONS_CONFIG_KEY, AGENT_MAX_NO_PROGRESS_ROUNDS_CONFIG_KEY,
    AGENT_MAX_REQUEST_RETRIES_CONFIG_KEY, AGENT_OUTPUT_RESERVE_TOKENS_CONFIG_KEY,
    AGENT_REQUEST_TIMEOUT_SECONDS_CONFIG_KEY, AGENT_RUN_TIMEOUT_SECONDS_CONFIG_KEY,
    DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_AGENT_MAX_NO_PROGRESS_ROUNDS, DEFAULT_AGENT_MAX_REQUEST_RETRIES,
    DEFAULT_AGENT_OUTPUT_RESERVE_TOKENS, DEFAULT_AGENT_REQUEST_TIMEOUT_SECONDS,
    DEFAULT_AGENT_RUN_TIMEOUT_SECONDS, DEFAULT_TASK_RUNNER_MAX_ITERATIONS,
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
    MemoryEngineAgent, MemoryEngineAgentKind, COMMAND_APPROVAL_AGENT,
    MEMORY_ENGINE_MEMORY_ROLLUP_AGENT, MEMORY_ENGINE_ROLLUP_AGENT,
    MEMORY_ENGINE_SUBJECT_MEMORY_AGENT, MEMORY_ENGINE_SUMMARY_AGENT,
    MEMORY_ENGINE_THREAD_REPAIR_AGENT,
};

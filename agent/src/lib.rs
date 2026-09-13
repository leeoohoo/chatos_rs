// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod catalog;

pub use catalog::{
    agent_descriptor, can_use_chatos_notepad, is_chatos_conversation_agent,
    is_task_runner_execution_agent, is_task_runner_phase_agent, parse_system_agent_key,
    system_agent_catalog, AgentDescriptor, AgentExecutionLocation,
};
pub use chatos_plugin_management_sdk::SystemAgentKey;

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{AgentToolPlane, SystemAgentKey};

pub const CHATOS_ASYNC_PLANNER_TOOL_PROFILE: &str = "chatos_async_planner";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatosTaskExecutionToolProfile {
    AsyncPlanner,
}

impl ChatosTaskExecutionToolProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AsyncPlanner => CHATOS_ASYNC_PLANNER_TOOL_PROFILE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentExecutionLocation {
    ServerOrchestrated,
    ClientEmbedded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDescriptor {
    pub key: SystemAgentKey,
    pub display_name: &'static str,
    pub service_name: &'static str,
    pub description: &'static str,
    pub include_user_resources: bool,
    pub tool_plane: AgentToolPlane,
    pub execution_location: AgentExecutionLocation,
}

impl AgentDescriptor {
    pub const fn new(
        key: SystemAgentKey,
        display_name: &'static str,
        service_name: &'static str,
        description: &'static str,
        include_user_resources: bool,
        tool_plane: AgentToolPlane,
        execution_location: AgentExecutionLocation,
    ) -> Self {
        Self {
            key,
            display_name,
            service_name,
            description,
            include_user_resources,
            tool_plane,
            execution_location,
        }
    }
}

pub static CHATOS_CONVERSATION_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::ChatosConversationAgent,
    "Chat OS Conversation Agent",
    "local-agent-host",
    "Runs normal Chat OS conversations locally while applying the selected contact as user-specific role context.",
    false,
    AgentToolPlane::Managed,
    AgentExecutionLocation::ClientEmbedded,
);

pub static LOCAL_AGENT_EXECUTION_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::LocalAgentExecutionAgent,
    "Local Agent Execution Agent",
    "local-agent-host",
    "Executes implementation, testing, repair, deployment, and other durable work on the local client.",
    true,
    AgentToolPlane::Managed,
    AgentExecutionLocation::ClientEmbedded,
);

pub static LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_DESCRIPTOR: AgentDescriptor =
    AgentDescriptor::new(
        SystemAgentKey::LocalConnectorCommandApprovalAgent,
        "Command Approval Agent",
        "local-connector-client",
        "Reviews local shell commands with read-only project tools and returns an approval decision.",
        false,
        AgentToolPlane::LocalOnly,
        AgentExecutionLocation::ClientEmbedded,
    );

pub static MEMORY_ENGINE_SUMMARY_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineSummaryAgent,
    "Memory Engine Message Summary Agent",
    "memory-engine",
    "Compresses raw conversation records into a high-signal level-zero thread summary.",
    false,
    AgentToolPlane::None,
    AgentExecutionLocation::ServerOrchestrated,
);

pub static MEMORY_ENGINE_ROLLUP_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineRollupAgent,
    "Memory Engine Summary Rollup Agent",
    "memory-engine",
    "Consolidates lower-level thread summaries into durable higher-level project knowledge.",
    false,
    AgentToolPlane::None,
    AgentExecutionLocation::ServerOrchestrated,
);

pub static MEMORY_ENGINE_SUBJECT_MEMORY_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineSubjectMemoryAgent,
    "Memory Engine Subject Memory Agent",
    "memory-engine",
    "Distills thread summaries into durable subject memories for long-term recall.",
    false,
    AgentToolPlane::None,
    AgentExecutionLocation::ServerOrchestrated,
);

pub static MEMORY_ENGINE_MEMORY_ROLLUP_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineMemoryRollupAgent,
    "Memory Engine Memory Rollup Agent",
    "memory-engine",
    "Consolidates lower-level subject memories into stable higher-level long-term memory.",
    false,
    AgentToolPlane::None,
    AgentExecutionLocation::ServerOrchestrated,
);

pub static MEMORY_ENGINE_THREAD_REPAIR_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineThreadRepairAgent,
    "Memory Engine Thread Repair Agent",
    "memory-engine",
    "Builds a user-grounded repair summary when conversation context has drifted.",
    false,
    AgentToolPlane::None,
    AgentExecutionLocation::ServerOrchestrated,
);

static SYSTEM_AGENT_CATALOG: [&AgentDescriptor; 8] = [
    &CHATOS_CONVERSATION_AGENT_DESCRIPTOR,
    &LOCAL_AGENT_EXECUTION_AGENT_DESCRIPTOR,
    &LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_DESCRIPTOR,
    &MEMORY_ENGINE_SUMMARY_AGENT_DESCRIPTOR,
    &MEMORY_ENGINE_ROLLUP_AGENT_DESCRIPTOR,
    &MEMORY_ENGINE_SUBJECT_MEMORY_AGENT_DESCRIPTOR,
    &MEMORY_ENGINE_MEMORY_ROLLUP_AGENT_DESCRIPTOR,
    &MEMORY_ENGINE_THREAD_REPAIR_AGENT_DESCRIPTOR,
];

pub fn system_agent_catalog() -> &'static [&'static AgentDescriptor] {
    &SYSTEM_AGENT_CATALOG
}

pub fn parse_system_agent_key(value: &str) -> Option<SystemAgentKey> {
    let normalized = value.trim();
    SystemAgentKey::ALL
        .into_iter()
        .find(|key| key.as_str() == normalized)
}

pub fn parse_chatos_task_execution_tool_profile(
    value: &str,
) -> Option<ChatosTaskExecutionToolProfile> {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case(CHATOS_ASYNC_PLANNER_TOOL_PROFILE) {
        Some(ChatosTaskExecutionToolProfile::AsyncPlanner)
    } else {
        None
    }
}

pub const fn is_chatos_callback_agent(key: SystemAgentKey) -> bool {
    matches!(key, SystemAgentKey::ChatosConversationAgent)
}

pub const fn is_local_task_execution_agent(key: SystemAgentKey) -> bool {
    matches!(key, SystemAgentKey::LocalAgentExecutionAgent)
}

pub const fn uses_chatos_notepad_callback(key: SystemAgentKey) -> bool {
    is_chatos_callback_agent(key) || is_local_task_execution_agent(key)
}

pub const fn uses_chatos_browser_callback(key: SystemAgentKey) -> bool {
    uses_chatos_notepad_callback(key)
}

pub const fn chatos_task_execution_tool_profile(key: SystemAgentKey) -> Option<&'static str> {
    if is_chatos_callback_agent(key) {
        Some(CHATOS_ASYNC_PLANNER_TOOL_PROFILE)
    } else {
        None
    }
}

pub fn agent_descriptor(key: SystemAgentKey) -> &'static AgentDescriptor {
    match key {
        SystemAgentKey::ChatosConversationAgent => &CHATOS_CONVERSATION_AGENT_DESCRIPTOR,
        SystemAgentKey::LocalAgentExecutionAgent => &LOCAL_AGENT_EXECUTION_AGENT_DESCRIPTOR,
        SystemAgentKey::LocalConnectorCommandApprovalAgent => {
            &LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_DESCRIPTOR
        }
        SystemAgentKey::MemoryEngineSummaryAgent => &MEMORY_ENGINE_SUMMARY_AGENT_DESCRIPTOR,
        SystemAgentKey::MemoryEngineRollupAgent => &MEMORY_ENGINE_ROLLUP_AGENT_DESCRIPTOR,
        SystemAgentKey::MemoryEngineSubjectMemoryAgent => {
            &MEMORY_ENGINE_SUBJECT_MEMORY_AGENT_DESCRIPTOR
        }
        SystemAgentKey::MemoryEngineMemoryRollupAgent => {
            &MEMORY_ENGINE_MEMORY_ROLLUP_AGENT_DESCRIPTOR
        }
        SystemAgentKey::MemoryEngineThreadRepairAgent => {
            &MEMORY_ENGINE_THREAD_REPAIR_AGENT_DESCRIPTOR
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn active_catalog_contains_each_registered_agent_once() {
        let keys = system_agent_catalog()
            .iter()
            .map(|descriptor| descriptor.key.as_str())
            .collect::<Vec<_>>();
        let unique = keys.iter().copied().collect::<HashSet<_>>();

        assert_eq!(keys.len(), 8);
        assert_eq!(unique.len(), keys.len());
        assert_eq!(
            keys,
            vec![
                "chatos_conversation_agent",
                "local_agent_execution_agent",
                "local_connector_command_approval_agent",
                "memory_engine_summary_agent",
                "memory_engine_rollup_agent",
                "memory_engine_subject_memory_agent",
                "memory_engine_memory_rollup_agent",
                "memory_engine_thread_repair_agent",
            ]
        );
    }

    #[test]
    fn only_memory_generation_agents_have_no_tool_plane() {
        let no_tool_plane = system_agent_catalog()
            .iter()
            .filter(|descriptor| descriptor.tool_plane == AgentToolPlane::None)
            .map(|descriptor| descriptor.key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            no_tool_plane,
            vec![
                "memory_engine_summary_agent",
                "memory_engine_rollup_agent",
                "memory_engine_subject_memory_agent",
                "memory_engine_memory_rollup_agent",
                "memory_engine_thread_repair_agent",
            ]
        );
        assert!(system_agent_catalog()
            .iter()
            .filter(|descriptor| descriptor.service_name != "memory-engine")
            .all(|descriptor| descriptor.tool_plane.supports_tools()));
    }

    #[test]
    fn local_command_approval_agent_never_uses_the_managed_gateway() {
        let descriptor = agent_descriptor(SystemAgentKey::LocalConnectorCommandApprovalAgent);

        assert_eq!(descriptor.tool_plane, AgentToolPlane::LocalOnly);
        assert!(descriptor.tool_plane.supports_tools());
        assert!(!descriptor.tool_plane.uses_managed_gateway());
    }

    #[test]
    fn callback_groups_live_with_agent_catalog() {
        assert!(is_chatos_callback_agent(
            SystemAgentKey::ChatosConversationAgent
        ));
        assert!(is_local_task_execution_agent(
            SystemAgentKey::LocalAgentExecutionAgent
        ));
        assert!(uses_chatos_notepad_callback(
            SystemAgentKey::LocalAgentExecutionAgent
        ));
        assert!(uses_chatos_browser_callback(
            SystemAgentKey::ChatosConversationAgent
        ));
        assert!(!uses_chatos_browser_callback(
            SystemAgentKey::MemoryEngineSummaryAgent
        ));
    }

    #[test]
    fn parser_and_chatos_semantics_are_centralized() {
        assert_eq!(parse_system_agent_key(" task_runner_plan_phase "), None);
        assert_eq!(parse_system_agent_key("unknown"), None);
        assert_eq!(
            parse_chatos_task_execution_tool_profile(" chatos_async_planner "),
            Some(ChatosTaskExecutionToolProfile::AsyncPlanner)
        );
        assert_eq!(
            chatos_task_execution_tool_profile(SystemAgentKey::ChatosConversationAgent),
            Some(CHATOS_ASYNC_PLANNER_TOOL_PROFILE)
        );
    }

    #[test]
    fn conversation_execution_and_approval_are_client_embedded_agents() {
        let local_loop_agents = system_agent_catalog()
            .iter()
            .filter(|descriptor| {
                descriptor.execution_location == AgentExecutionLocation::ClientEmbedded
            })
            .map(|descriptor| descriptor.key)
            .collect::<Vec<_>>();

        assert_eq!(
            local_loop_agents,
            vec![
                SystemAgentKey::ChatosConversationAgent,
                SystemAgentKey::LocalAgentExecutionAgent,
                SystemAgentKey::LocalConnectorCommandApprovalAgent,
            ]
        );
        assert_eq!(
            system_agent_catalog()
                .iter()
                .filter(|descriptor| {
                    descriptor.execution_location == AgentExecutionLocation::ServerOrchestrated
                })
                .count(),
            system_agent_catalog().len() - local_loop_agents.len()
        );
    }
}

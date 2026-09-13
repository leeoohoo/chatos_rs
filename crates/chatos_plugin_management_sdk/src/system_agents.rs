// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{AgentToolPlane, SystemAgentKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentExecutionLocation {
    MemoryEngineOwned,
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
    const fn new(
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

static SYSTEM_AGENT_CATALOG: [AgentDescriptor; 8] = [
    AgentDescriptor::new(
        SystemAgentKey::ChatosConversationAgent,
        "Chat OS Conversation Agent",
        "local-agent-host",
        "Runs client-local Chat OS conversations while applying the selected contact as user-specific role context.",
        false,
        AgentToolPlane::Managed,
        AgentExecutionLocation::ClientEmbedded,
    ),
    AgentDescriptor::new(
        SystemAgentKey::TaskRunnerRunPhase,
        "Task Runner Execution Agent",
        "local-agent-host",
        "Executes implementation, testing, repair, deployment, and other mutating work in the client-local Task Runner profile.",
        true,
        AgentToolPlane::Managed,
        AgentExecutionLocation::ClientEmbedded,
    ),
    AgentDescriptor::new(
        SystemAgentKey::LocalConnectorCommandApprovalAgent,
        "Command Approval Agent",
        "local-connector-client",
        "Reviews local shell commands with read-only project tools and returns an approval decision.",
        false,
        AgentToolPlane::LocalOnly,
        AgentExecutionLocation::ClientEmbedded,
    ),
    AgentDescriptor::new(
        SystemAgentKey::MemoryEngineSummaryAgent,
        "Memory Engine Message Summary Agent",
        "memory-engine",
        "Compresses raw conversation records into a high-signal level-zero thread summary.",
        false,
        AgentToolPlane::None,
        AgentExecutionLocation::MemoryEngineOwned,
    ),
    AgentDescriptor::new(
        SystemAgentKey::MemoryEngineRollupAgent,
        "Memory Engine Summary Rollup Agent",
        "memory-engine",
        "Consolidates lower-level thread summaries into durable higher-level project knowledge.",
        false,
        AgentToolPlane::None,
        AgentExecutionLocation::MemoryEngineOwned,
    ),
    AgentDescriptor::new(
        SystemAgentKey::MemoryEngineSubjectMemoryAgent,
        "Memory Engine Subject Memory Agent",
        "memory-engine",
        "Distills thread summaries into durable subject memories for long-term recall.",
        false,
        AgentToolPlane::None,
        AgentExecutionLocation::MemoryEngineOwned,
    ),
    AgentDescriptor::new(
        SystemAgentKey::MemoryEngineMemoryRollupAgent,
        "Memory Engine Memory Rollup Agent",
        "memory-engine",
        "Consolidates lower-level subject memories into stable higher-level long-term memory.",
        false,
        AgentToolPlane::None,
        AgentExecutionLocation::MemoryEngineOwned,
    ),
    AgentDescriptor::new(
        SystemAgentKey::MemoryEngineThreadRepairAgent,
        "Memory Engine Thread Repair Agent",
        "memory-engine",
        "Builds a user-grounded repair summary when conversation context has drifted.",
        false,
        AgentToolPlane::None,
        AgentExecutionLocation::MemoryEngineOwned,
    ),
];

pub fn system_agent_catalog() -> &'static [AgentDescriptor] {
    &SYSTEM_AGENT_CATALOG
}

pub fn parse_system_agent_key(value: &str) -> Option<SystemAgentKey> {
    let normalized = value.trim();
    SystemAgentKey::ALL
        .into_iter()
        .find(|key| key.as_str() == normalized)
}

pub const fn is_chatos_conversation_agent(key: SystemAgentKey) -> bool {
    matches!(key, SystemAgentKey::ChatosConversationAgent)
}

pub const fn is_task_runner_phase_agent(key: SystemAgentKey) -> bool {
    matches!(key, SystemAgentKey::TaskRunnerRunPhase)
}

pub const fn is_task_runner_execution_agent(key: SystemAgentKey) -> bool {
    is_task_runner_phase_agent(key)
}

pub const fn can_use_chatos_notepad(key: SystemAgentKey) -> bool {
    is_chatos_conversation_agent(key) || is_task_runner_phase_agent(key)
}

pub fn agent_descriptor(key: SystemAgentKey) -> &'static AgentDescriptor {
    SYSTEM_AGENT_CATALOG
        .iter()
        .find(|descriptor| descriptor.key == key)
        .expect("system agent key must have a catalog descriptor")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_contains_all_system_agent_keys_once() {
        let keys = system_agent_catalog()
            .iter()
            .map(|descriptor| descriptor.key)
            .collect::<Vec<_>>();
        let unique = keys.iter().copied().collect::<HashSet<_>>();

        assert_eq!(keys.len(), SystemAgentKey::ALL.len());
        assert_eq!(unique.len(), keys.len());
        assert_eq!(keys.as_slice(), SystemAgentKey::ALL.as_slice());
    }

    #[test]
    fn memory_engine_agents_are_the_only_server_orchestrated_toolless_agents() {
        let server_agents = system_agent_catalog()
            .iter()
            .filter(|descriptor| {
                descriptor.execution_location == AgentExecutionLocation::MemoryEngineOwned
            })
            .collect::<Vec<_>>();

        assert_eq!(server_agents.len(), 5);
        assert!(server_agents
            .iter()
            .all(|descriptor| descriptor.service_name == "memory-engine"
                && descriptor.tool_plane == AgentToolPlane::None));
        assert!(system_agent_catalog()
            .iter()
            .filter(|descriptor| {
                descriptor.execution_location == AgentExecutionLocation::ClientEmbedded
            })
            .all(|descriptor| descriptor.tool_plane.supports_tools()));
    }
}

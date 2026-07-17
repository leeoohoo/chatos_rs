// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemAgentKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDescriptor {
    pub key: SystemAgentKey,
    pub display_name: &'static str,
    pub service_name: &'static str,
    pub description: &'static str,
    pub include_user_resources: bool,
}

impl AgentDescriptor {
    pub const fn new(
        key: SystemAgentKey,
        display_name: &'static str,
        service_name: &'static str,
        description: &'static str,
        include_user_resources: bool,
    ) -> Self {
        Self {
            key,
            display_name,
            service_name,
            description,
            include_user_resources,
        }
    }
}

pub static CHATOS_CONVERSATION_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::ChatosConversationAgent,
    "Chat OS Conversation Agent",
    "chatos",
    "Runs normal Chat OS conversations while applying the selected contact as user-specific role context.",
    false,
);

pub static CHATOS_PLANNING_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::ChatosPlanningAgent,
    "Chat OS Planning Agent",
    "chatos",
    "Runs Chat OS plan mode and requires the Task Runner MCP with the chatos_plan task profile.",
    false,
);

pub static PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_DESCRIPTOR: AgentDescriptor =
    AgentDescriptor::new(
        SystemAgentKey::ProjectRequirementExecutionPlannerAgent,
        "Project Requirement Execution Planner Agent",
        "chatos",
        "Splits project-management work items into concrete Task Runner execution tasks for Chat OS project requirement execution.",
        true,
    );

pub static TASK_RUNNER_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::TaskRunnerRunPhase,
    "Task Runner Agent",
    "task-runner",
    "Runs both default and chatos_plan task profiles through the same model and tool loop.",
    true,
);

pub static PROJECT_MANAGEMENT_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::ProjectManagementAgent,
    "Project Runtime Environment Agent",
    "project-service",
    "Inspects project files, resolves sandbox images, and persists the project runtime environment.",
    false,
);

pub static LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_DESCRIPTOR: AgentDescriptor =
    AgentDescriptor::new(
        SystemAgentKey::LocalConnectorCommandApprovalAgent,
        "Local Command Approval Agent",
        "local-connector-client",
        "Reviews local shell commands with read-only project tools and returns an approval decision.",
        false,
    );

pub static MEMORY_ENGINE_SUMMARY_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineSummaryAgent,
    "Memory Engine Message Summary Agent",
    "memory-engine",
    "Compresses raw conversation records into a high-signal level-zero thread summary.",
    false,
);

pub static MEMORY_ENGINE_ROLLUP_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineRollupAgent,
    "Memory Engine Summary Rollup Agent",
    "memory-engine",
    "Consolidates lower-level thread summaries into durable higher-level project knowledge.",
    false,
);

pub static MEMORY_ENGINE_SUBJECT_MEMORY_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineSubjectMemoryAgent,
    "Memory Engine Subject Memory Agent",
    "memory-engine",
    "Distills thread summaries into durable subject memories for long-term recall.",
    false,
);

pub static MEMORY_ENGINE_MEMORY_ROLLUP_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineMemoryRollupAgent,
    "Memory Engine Memory Rollup Agent",
    "memory-engine",
    "Consolidates lower-level subject memories into stable higher-level long-term memory.",
    false,
);

pub static MEMORY_ENGINE_THREAD_REPAIR_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineThreadRepairAgent,
    "Memory Engine Thread Repair Agent",
    "memory-engine",
    "Builds a user-grounded repair summary when conversation context has drifted.",
    false,
);

static SYSTEM_AGENT_CATALOG: [&AgentDescriptor; 11] = [
    &CHATOS_CONVERSATION_AGENT_DESCRIPTOR,
    &CHATOS_PLANNING_AGENT_DESCRIPTOR,
    &PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_DESCRIPTOR,
    &TASK_RUNNER_AGENT_DESCRIPTOR,
    &PROJECT_MANAGEMENT_AGENT_DESCRIPTOR,
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

pub fn agent_descriptor(key: SystemAgentKey) -> &'static AgentDescriptor {
    match key {
        SystemAgentKey::ChatosConversationAgent => &CHATOS_CONVERSATION_AGENT_DESCRIPTOR,
        SystemAgentKey::ChatosPlanningAgent => &CHATOS_PLANNING_AGENT_DESCRIPTOR,
        SystemAgentKey::ProjectRequirementExecutionPlannerAgent => {
            &PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_DESCRIPTOR
        }
        SystemAgentKey::TaskRunnerRunPhase => &TASK_RUNNER_AGENT_DESCRIPTOR,
        SystemAgentKey::ProjectManagementAgent => &PROJECT_MANAGEMENT_AGENT_DESCRIPTOR,
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
    fn catalog_contains_all_system_agent_keys_once() {
        let keys = system_agent_catalog()
            .iter()
            .map(|descriptor| descriptor.key.as_str())
            .collect::<Vec<_>>();
        let unique = keys.iter().copied().collect::<HashSet<_>>();

        assert_eq!(keys.len(), 11);
        assert_eq!(unique.len(), keys.len());
        assert_eq!(
            keys,
            vec![
                "chatos_conversation_agent",
                "chatos_planning_agent",
                "project_requirement_execution_planner_agent",
                "task_runner_run_phase",
                "project_management_agent",
                "local_connector_command_approval_agent",
                "memory_engine_summary_agent",
                "memory_engine_rollup_agent",
                "memory_engine_subject_memory_agent",
                "memory_engine_memory_rollup_agent",
                "memory_engine_thread_repair_agent",
            ]
        );
    }
}

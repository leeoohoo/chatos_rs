// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{AgentToolPlane, SystemAgentKey};

pub const CHATOS_ASYNC_PLANNER_TOOL_PROFILE: &str = "chatos_async_planner";
pub const PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE: &str =
    "project_requirement_execution_planner";
pub const CHATOS_PLAN_TASK_PROFILE: &str = "chatos_plan";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatosTaskRunnerToolProfile {
    AsyncPlanner,
    ProjectRequirementExecutionPlanner,
}

impl ChatosTaskRunnerToolProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AsyncPlanner => CHATOS_ASYNC_PLANNER_TOOL_PROFILE,
            Self::ProjectRequirementExecutionPlanner => {
                PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDescriptor {
    pub key: SystemAgentKey,
    pub display_name: &'static str,
    pub service_name: &'static str,
    pub description: &'static str,
    pub include_user_resources: bool,
    pub tool_plane: AgentToolPlane,
}

impl AgentDescriptor {
    pub const fn new(
        key: SystemAgentKey,
        display_name: &'static str,
        service_name: &'static str,
        description: &'static str,
        include_user_resources: bool,
        tool_plane: AgentToolPlane,
    ) -> Self {
        Self {
            key,
            display_name,
            service_name,
            description,
            include_user_resources,
            tool_plane,
        }
    }
}

pub static CHATOS_CONVERSATION_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::ChatosConversationAgent,
    "Chat OS Conversation Agent",
    "chatos",
    "Runs normal Chat OS conversations while applying the selected contact as user-specific role context.",
    false,
    AgentToolPlane::Managed,
);

static RETIRED_CHATOS_PLANNING_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::ChatosPlanningAgent,
    "Retired Chat OS Planning Agent",
    "chatos",
    "Retired compatibility identity. Chat OS plan mode now submits a Task Runner planning task programmatically, and task_runner_plan_phase performs the planning.",
    false,
    AgentToolPlane::None,
);

pub static PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_DESCRIPTOR: AgentDescriptor =
    AgentDescriptor::new(
        SystemAgentKey::ProjectRequirementExecutionPlannerAgent,
        "Project Requirement Execution Planner Agent",
        "chatos",
        "Splits project-management work items into concrete Task Runner execution tasks for Chat OS project requirement execution.",
        true,
        AgentToolPlane::Managed,
    );

pub static TASK_RUNNER_PLAN_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::TaskRunnerPlanPhase,
    "Cloud Task Runner Planning Agent",
    "task-runner",
    "Runs non-mutating Task Runner planning tasks in the cloud execution plane with a planning-specific Prompt and capability boundary.",
    true,
    AgentToolPlane::Managed,
);

static RETIRED_TASK_RUNNER_LOCAL_PLAN_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::TaskRunnerLocalPlanPhase,
    "Retired Local Task Runner Planning Agent",
    "task-runner",
    "Retired compatibility identity. Local projects use the cloud Task Runner planning Agent and route local tools through Local Connector.",
    false,
    AgentToolPlane::None,
);

pub static TASK_RUNNER_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::TaskRunnerRunPhase,
    "Cloud Task Runner Execution Agent",
    "task-runner",
    "Executes implementation, testing, repair, deployment, and other mutating Task Runner work in the cloud execution plane.",
    true,
    AgentToolPlane::Managed,
);

static RETIRED_TASK_RUNNER_LOCAL_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::TaskRunnerLocalRunPhase,
    "Retired Local Task Runner Execution Agent",
    "task-runner",
    "Retired compatibility identity. Local projects use the cloud Task Runner execution Agent and route local tools through Local Connector.",
    false,
    AgentToolPlane::None,
);

pub static PROJECT_MANAGEMENT_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::ProjectManagementAgent,
    "Project Runtime Environment Agent",
    "project-service",
    "Inspects project files, resolves sandbox images, and persists the project runtime environment.",
    false,
    AgentToolPlane::Managed,
);

pub static LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_DESCRIPTOR: AgentDescriptor =
    AgentDescriptor::new(
        SystemAgentKey::LocalConnectorCommandApprovalAgent,
        "Local Command Approval Agent",
        "local-connector-client",
        "Reviews local shell commands with read-only project tools and returns an approval decision.",
        false,
        AgentToolPlane::LocalOnly,
    );

pub static MEMORY_ENGINE_SUMMARY_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineSummaryAgent,
    "Memory Engine Message Summary Agent",
    "memory-engine",
    "Compresses raw conversation records into a high-signal level-zero thread summary.",
    false,
    AgentToolPlane::None,
);

pub static MEMORY_ENGINE_ROLLUP_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineRollupAgent,
    "Memory Engine Summary Rollup Agent",
    "memory-engine",
    "Consolidates lower-level thread summaries into durable higher-level project knowledge.",
    false,
    AgentToolPlane::None,
);

pub static MEMORY_ENGINE_SUBJECT_MEMORY_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineSubjectMemoryAgent,
    "Memory Engine Subject Memory Agent",
    "memory-engine",
    "Distills thread summaries into durable subject memories for long-term recall.",
    false,
    AgentToolPlane::None,
);

pub static MEMORY_ENGINE_MEMORY_ROLLUP_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineMemoryRollupAgent,
    "Memory Engine Memory Rollup Agent",
    "memory-engine",
    "Consolidates lower-level subject memories into stable higher-level long-term memory.",
    false,
    AgentToolPlane::None,
);

pub static MEMORY_ENGINE_THREAD_REPAIR_AGENT_DESCRIPTOR: AgentDescriptor = AgentDescriptor::new(
    SystemAgentKey::MemoryEngineThreadRepairAgent,
    "Memory Engine Thread Repair Agent",
    "memory-engine",
    "Builds a user-grounded repair summary when conversation context has drifted.",
    false,
    AgentToolPlane::None,
);

static SYSTEM_AGENT_CATALOG: [&AgentDescriptor; 11] = [
    &CHATOS_CONVERSATION_AGENT_DESCRIPTOR,
    &PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_DESCRIPTOR,
    &TASK_RUNNER_PLAN_AGENT_DESCRIPTOR,
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

pub fn parse_system_agent_key(value: &str) -> Option<SystemAgentKey> {
    let normalized = value.trim();
    SystemAgentKey::ALL
        .into_iter()
        .find(|key| key.as_str() == normalized)
}

pub fn parse_chatos_task_runner_tool_profile(value: &str) -> Option<ChatosTaskRunnerToolProfile> {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE) {
        Some(ChatosTaskRunnerToolProfile::ProjectRequirementExecutionPlanner)
    } else if normalized.eq_ignore_ascii_case(CHATOS_ASYNC_PLANNER_TOOL_PROFILE) {
        Some(ChatosTaskRunnerToolProfile::AsyncPlanner)
    } else {
        None
    }
}

pub fn is_chatos_plan_task_profile(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case(CHATOS_PLAN_TASK_PROFILE)
}

pub const fn is_chatos_callback_agent(key: SystemAgentKey) -> bool {
    matches!(
        key,
        SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
    )
}

pub const fn is_project_requirement_execution_planner_agent(key: SystemAgentKey) -> bool {
    matches!(key, SystemAgentKey::ProjectRequirementExecutionPlannerAgent)
}

pub const fn is_task_runner_phase_agent(key: SystemAgentKey) -> bool {
    matches!(
        key,
        SystemAgentKey::TaskRunnerPlanPhase | SystemAgentKey::TaskRunnerRunPhase
    )
}

pub const fn is_task_runner_planning_agent(key: SystemAgentKey) -> bool {
    matches!(key, SystemAgentKey::TaskRunnerPlanPhase)
}

pub const fn is_task_runner_execution_agent(key: SystemAgentKey) -> bool {
    matches!(key, SystemAgentKey::TaskRunnerRunPhase)
}

pub const fn uses_chatos_notepad_callback(key: SystemAgentKey) -> bool {
    is_chatos_callback_agent(key) || is_task_runner_phase_agent(key)
}

pub const fn uses_chatos_browser_callback(key: SystemAgentKey) -> bool {
    uses_chatos_notepad_callback(key)
}

pub const fn chatos_task_runner_tool_profile(key: SystemAgentKey) -> Option<&'static str> {
    if is_project_requirement_execution_planner_agent(key) {
        Some(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE)
    } else if is_chatos_callback_agent(key) {
        Some(CHATOS_ASYNC_PLANNER_TOOL_PROFILE)
    } else {
        None
    }
}

pub const fn requires_expected_project_task_ids(key: SystemAgentKey) -> bool {
    is_project_requirement_execution_planner_agent(key)
}

pub fn agent_descriptor(key: SystemAgentKey) -> &'static AgentDescriptor {
    match key {
        SystemAgentKey::ChatosConversationAgent => &CHATOS_CONVERSATION_AGENT_DESCRIPTOR,
        SystemAgentKey::ChatosPlanningAgent => &RETIRED_CHATOS_PLANNING_AGENT_DESCRIPTOR,
        SystemAgentKey::ProjectRequirementExecutionPlannerAgent => {
            &PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_DESCRIPTOR
        }
        SystemAgentKey::TaskRunnerPlanPhase => &TASK_RUNNER_PLAN_AGENT_DESCRIPTOR,
        SystemAgentKey::TaskRunnerLocalPlanPhase => {
            &RETIRED_TASK_RUNNER_LOCAL_PLAN_AGENT_DESCRIPTOR
        }
        SystemAgentKey::TaskRunnerRunPhase => &TASK_RUNNER_AGENT_DESCRIPTOR,
        SystemAgentKey::TaskRunnerLocalRunPhase => &RETIRED_TASK_RUNNER_LOCAL_AGENT_DESCRIPTOR,
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
                "project_requirement_execution_planner_agent",
                "task_runner_plan_phase",
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
        for key in [
            SystemAgentKey::ChatosConversationAgent,
            SystemAgentKey::ChatosPlanningAgent,
            SystemAgentKey::ProjectRequirementExecutionPlannerAgent,
        ] {
            assert!(is_chatos_callback_agent(key));
        }
        assert!(is_task_runner_phase_agent(
            SystemAgentKey::TaskRunnerPlanPhase
        ));
        assert!(is_task_runner_planning_agent(
            SystemAgentKey::TaskRunnerPlanPhase
        ));
        assert!(is_task_runner_execution_agent(
            SystemAgentKey::TaskRunnerRunPhase
        ));
        assert!(uses_chatos_notepad_callback(
            SystemAgentKey::TaskRunnerRunPhase
        ));
        assert!(uses_chatos_browser_callback(
            SystemAgentKey::ChatosConversationAgent
        ));
        assert!(!uses_chatos_notepad_callback(
            SystemAgentKey::ProjectManagementAgent
        ));
        assert!(!uses_chatos_browser_callback(
            SystemAgentKey::MemoryEngineSummaryAgent
        ));
    }

    #[test]
    fn parser_and_chatos_semantics_are_centralized() {
        assert_eq!(
            parse_system_agent_key(" task_runner_plan_phase "),
            Some(SystemAgentKey::TaskRunnerPlanPhase)
        );
        assert_eq!(parse_system_agent_key("unknown"), None);
        assert_eq!(
            parse_chatos_task_runner_tool_profile(" chatos_async_planner "),
            Some(ChatosTaskRunnerToolProfile::AsyncPlanner)
        );
        assert_eq!(
            parse_chatos_task_runner_tool_profile("project_requirement_execution_planner"),
            Some(ChatosTaskRunnerToolProfile::ProjectRequirementExecutionPlanner)
        );
        assert!(is_chatos_plan_task_profile(" chatos_plan "));
        assert!(!is_chatos_plan_task_profile("default"));
        assert_eq!(
            chatos_task_runner_tool_profile(SystemAgentKey::ChatosConversationAgent),
            Some(CHATOS_ASYNC_PLANNER_TOOL_PROFILE)
        );
        assert_eq!(
            chatos_task_runner_tool_profile(SystemAgentKey::ChatosPlanningAgent),
            Some(CHATOS_ASYNC_PLANNER_TOOL_PROFILE)
        );
        assert_eq!(
            chatos_task_runner_tool_profile(
                SystemAgentKey::ProjectRequirementExecutionPlannerAgent
            ),
            Some(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE)
        );
        assert!(requires_expected_project_task_ids(
            SystemAgentKey::ProjectRequirementExecutionPlannerAgent
        ));
        assert!(!requires_expected_project_task_ids(
            SystemAgentKey::ChatosConversationAgent
        ));
    }
}

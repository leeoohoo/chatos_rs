// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemAgentKey;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity, SystemAgentDefinition};

pub const PROJECT_ENVIRONMENT_AGENT: ProjectEnvironmentAgent =
    ProjectEnvironmentAgent::new(SystemAgentKey::ProjectManagementAgent);
pub const PROJECT_ENVIRONMENT_LOCAL_AGENT: ProjectEnvironmentAgent =
    ProjectEnvironmentAgent::new(SystemAgentKey::ProjectManagementLocalAgent);

#[derive(Debug, Clone, Copy)]
pub struct ProjectEnvironmentAgent {
    key: SystemAgentKey,
}

impl ProjectEnvironmentAgent {
    pub const fn new(key: SystemAgentKey) -> Self {
        Self { key }
    }

    pub const fn for_project_locality(local_project: bool) -> Self {
        if local_project {
            PROJECT_ENVIRONMENT_LOCAL_AGENT
        } else {
            PROJECT_ENVIRONMENT_AGENT
        }
    }

    pub const fn key(self) -> SystemAgentKey {
        self.key
    }
}

impl Default for ProjectEnvironmentAgent {
    fn default() -> Self {
        PROJECT_ENVIRONMENT_AGENT
    }
}

impl AgentIdentity for ProjectEnvironmentAgent {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(self.key)
    }
}

impl SystemAgentDefinition for ProjectEnvironmentAgent {
    fn message_mode(&self) -> &'static str {
        "project_environment_agent"
    }

    fn message_source(&self) -> &'static str {
        "project_management_service"
    }

    fn context_overflow_trigger(&self) -> &'static str {
        "project_environment_agent_context_overflow"
    }

    fn default_temperature(&self) -> Option<f64> {
        Some(0.1)
    }

    fn default_max_output_tokens(&self) -> Option<i64> {
        Some(4_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_projects_receive_distinct_environment_agent_identity() {
        assert_eq!(
            ProjectEnvironmentAgent::for_project_locality(false).key(),
            SystemAgentKey::ProjectManagementAgent
        );
        assert_eq!(
            ProjectEnvironmentAgent::for_project_locality(true).key(),
            SystemAgentKey::ProjectManagementLocalAgent
        );
    }
}

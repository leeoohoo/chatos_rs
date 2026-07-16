// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemAgentKey;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity, SystemAgentDefinition};

pub const PROJECT_ENVIRONMENT_AGENT: ProjectEnvironmentAgent = ProjectEnvironmentAgent;

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectEnvironmentAgent;

impl AgentIdentity for ProjectEnvironmentAgent {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(SystemAgentKey::ProjectManagementAgent)
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

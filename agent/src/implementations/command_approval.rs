// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemAgentKey;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity, SystemAgentDefinition};

pub const COMMAND_APPROVAL_AGENT: CommandApprovalAgent = CommandApprovalAgent;

#[derive(Debug, Default, Clone, Copy)]
pub struct CommandApprovalAgent;

impl AgentIdentity for CommandApprovalAgent {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(SystemAgentKey::LocalConnectorCommandApprovalAgent)
    }
}

impl SystemAgentDefinition for CommandApprovalAgent {
    fn message_mode(&self) -> &'static str {
        "local_connector_command_approval_agent"
    }

    fn message_source(&self) -> &'static str {
        "local_connector_client"
    }

    fn context_overflow_trigger(&self) -> &'static str {
        "local_connector_command_approval_context_overflow"
    }

    fn default_temperature(&self) -> Option<f64> {
        Some(0.0)
    }

    fn default_max_output_tokens(&self) -> Option<i64> {
        Some(1_200)
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemMcpKey;

use crate::CatalogSystemMcpDefinition;

pub mod builtin;
pub mod sandbox_images;

static SYSTEM_MCP_DEFINITIONS: [CatalogSystemMcpDefinition; 19] = [
    CatalogSystemMcpDefinition::new(SystemMcpKey::CodeMaintainerRead),
    CatalogSystemMcpDefinition::new(SystemMcpKey::CodeMaintainerWrite),
    CatalogSystemMcpDefinition::new(SystemMcpKey::TerminalController),
    CatalogSystemMcpDefinition::new(SystemMcpKey::TaskManager),
    CatalogSystemMcpDefinition::new(SystemMcpKey::ProjectManagement),
    CatalogSystemMcpDefinition::new(SystemMcpKey::Notepad),
    CatalogSystemMcpDefinition::new(SystemMcpKey::AgentBuilder),
    CatalogSystemMcpDefinition::new(SystemMcpKey::AskUser),
    CatalogSystemMcpDefinition::new(SystemMcpKey::RemoteConnectionController),
    CatalogSystemMcpDefinition::new(SystemMcpKey::WebTools),
    CatalogSystemMcpDefinition::new(SystemMcpKey::BrowserTools),
    CatalogSystemMcpDefinition::new(SystemMcpKey::MemorySkillReader),
    CatalogSystemMcpDefinition::new(SystemMcpKey::MemoryCommandReader),
    CatalogSystemMcpDefinition::new(SystemMcpKey::MemoryPluginReader),
    CatalogSystemMcpDefinition::new(SystemMcpKey::SandboxImages),
    CatalogSystemMcpDefinition::new(SystemMcpKey::ProjectEnvironment),
    CatalogSystemMcpDefinition::new(SystemMcpKey::ProjectRuntimeEnvironment),
    CatalogSystemMcpDefinition::new(SystemMcpKey::LocalCommandApproval),
    CatalogSystemMcpDefinition::new(SystemMcpKey::TaskRunnerService),
];

pub fn system_mcp_definitions() -> &'static [CatalogSystemMcpDefinition] {
    &SYSTEM_MCP_DEFINITIONS
}

pub fn system_mcp_definition(key: SystemMcpKey) -> &'static CatalogSystemMcpDefinition {
    &SYSTEM_MCP_DEFINITIONS[SystemMcpKey::ALL
        .iter()
        .position(|candidate| *candidate == key)
        .expect("every SystemMcpKey must have a definition")]
}

#[cfg(test)]
mod tests {
    use crate::SystemMcpDefinition;

    use super::*;

    #[test]
    fn every_system_mcp_has_a_definition_object() {
        assert_eq!(system_mcp_definitions().len(), SystemMcpKey::ALL.len());
        for key in SystemMcpKey::ALL {
            let definition = system_mcp_definition(key);
            assert_eq!(definition.key(), key);
            assert_eq!(definition.descriptor().key, key);
        }
    }
}

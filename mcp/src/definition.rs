// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemMcpKey;

use crate::{
    system_mcp_descriptor, system_mcp_provider_skills, system_mcp_tool_catalog,
    SystemMcpDescriptor, SystemMcpProviderSkill, SystemMcpToolCatalog,
};

pub trait SystemMcpDefinition: Send + Sync {
    fn key(&self) -> SystemMcpKey;

    fn descriptor(&self) -> &'static SystemMcpDescriptor {
        system_mcp_descriptor(self.key())
    }

    fn tool_catalog(&self) -> Result<SystemMcpToolCatalog, String> {
        system_mcp_tool_catalog(self.key())
    }

    fn provider_skills(&self) -> Vec<SystemMcpProviderSkill> {
        system_mcp_provider_skills(self.key())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CatalogSystemMcpDefinition {
    key: SystemMcpKey,
}

impl CatalogSystemMcpDefinition {
    pub const fn new(key: SystemMcpKey) -> Self {
        Self { key }
    }
}

impl SystemMcpDefinition for CatalogSystemMcpDefinition {
    fn key(&self) -> SystemMcpKey {
        self.key
    }
}

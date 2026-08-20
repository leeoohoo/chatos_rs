// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use super::components::{
    PluginAgent, PluginApp, PluginAuthor, PluginCommand, PluginDependencySpec, PluginHook,
    PluginInterfaceMetadata, PluginMcpServer, PluginPathRef, PluginPermissionRequirement,
    PluginUiContribution,
};

pub const PLUGIN_MANIFEST_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: PluginAuthor,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub skills: Vec<PluginPathRef>,
    #[serde(default)]
    pub mcp_servers: Vec<PluginMcpServer>,
    #[serde(default)]
    pub apps: Vec<PluginApp>,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    #[serde(default)]
    pub agents: Vec<PluginAgent>,
    #[serde(default)]
    pub hooks: Vec<PluginHook>,
    #[serde(default)]
    pub ui: Vec<PluginUiContribution>,
    pub interface: PluginInterfaceMetadata,
    #[serde(default)]
    pub dependencies: PluginDependencySpec,
    #[serde(default)]
    pub permissions: Vec<PluginPermissionRequirement>,
}

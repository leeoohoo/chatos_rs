// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::components::{
    PluginAgent, PluginApp, PluginAuthor, PluginCommand, PluginDependencySpec, PluginHook,
    PluginInterfaceMetadata, PluginMcpServer, PluginPathRef, PluginPermissionRequirement,
    PluginUiContribution,
};

pub const PLUGIN_MANIFEST_SCHEMA_VERSION_V1: u32 = 1;
pub const PLUGIN_MANIFEST_SCHEMA_VERSION_V2: u32 = 2;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PluginExecutionHost {
    Cloud,
    #[default]
    Local,
    Portable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginExecutionPolicy {
    #[serde(default)]
    pub default_host: PluginExecutionHost,
    #[serde(default)]
    pub component_hosts: BTreeMap<String, PluginExecutionHost>,
    #[serde(skip)]
    implicit_v1: bool,
}

impl Default for PluginExecutionPolicy {
    fn default() -> Self {
        Self {
            default_host: PluginExecutionHost::Local,
            component_hosts: BTreeMap::new(),
            implicit_v1: true,
        }
    }
}

impl PluginExecutionPolicy {
    pub fn explicit(
        default_host: PluginExecutionHost,
        component_hosts: BTreeMap<String, PluginExecutionHost>,
    ) -> Self {
        Self {
            default_host,
            component_hosts,
            implicit_v1: false,
        }
    }

    pub fn host_for(&self, component_key: &str) -> PluginExecutionHost {
        self.component_hosts
            .get(component_key)
            .copied()
            .unwrap_or(self.default_host)
    }

    pub fn is_implicit_v1(&self) -> bool {
        self.implicit_v1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "PluginExecutionPolicy::is_implicit_v1")]
    pub execution: PluginExecutionPolicy,
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
    #[serde(default)]
    pub bundled_content_variant: Option<String>,
}

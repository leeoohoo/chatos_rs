// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::SystemAgentKey;

pub const PLUGIN_COMMAND_MAX_ALLOWED_TOOLS: usize = 128;
pub const PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES: usize = 256;
pub const PLUGIN_AGENT_DEFAULT_MAX_ITERATIONS: usize = 600;
pub const PLUGIN_AGENT_MAX_ITERATIONS: usize = 5_000;
pub const PLUGIN_UI_MAX_ASSETS: usize = 256;
pub const PLUGIN_UI_MAX_BRIDGE_CAPABILITIES: usize = 32;
pub const PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES: usize = 32;
pub const PLUGIN_UI_SURFACE_DETAIL_PANEL: &str = "detail_panel";
pub const PLUGIN_UI_SURFACE_MESSAGE_PANEL: &str = "message_panel";
pub const PLUGIN_UI_SURFACE_WORKBENCH: &str = "workbench";
pub const PLUGIN_UI_SURFACE_ARTIFACT_VIEWER: &str = "artifact_viewer";
pub const PLUGIN_UI_RUNTIME_DEFAULT_HEALTH_PATH: &str = "/api/health";
pub const PLUGIN_UI_RUNTIME_DEFAULT_LAUNCH_TIMEOUT_MS: u64 = 15_000;
pub const PLUGIN_UI_RUNTIME_MIN_LAUNCH_TIMEOUT_MS: u64 = 100;
pub const PLUGIN_UI_RUNTIME_MAX_LAUNCH_TIMEOUT_MS: u64 = 120_000;
pub const PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ: &str = "host.context.read";
pub const PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST: &str = "artifact.list";
pub const PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ: &str = "artifact.read";
pub const PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD: &str = "artifact.download";
pub const PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE: &str = "artifact.create";
pub const PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE: &str = "artifact.update";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginPathRef {
    pub path: String,
}

impl PluginPathRef {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginComponentKind {
    SkillCollection,
    McpServer,
    ConnectedApp,
    Command,
    Agent,
    HookSet,
    UiContribution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginInterfaceMetadata {
    pub display_name: String,
    pub short_description: String,
    pub long_description: String,
    pub developer_name: String,
    pub category: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    #[serde(rename = "websiteURL")]
    pub website_url: Option<String>,
    #[serde(default)]
    #[serde(rename = "privacyPolicyURL")]
    pub privacy_policy_url: Option<String>,
    #[serde(default)]
    #[serde(rename = "termsOfServiceURL")]
    pub terms_of_service_url: Option<String>,
    #[serde(default)]
    pub default_prompt: Vec<String>,
    #[serde(default)]
    pub brand_color: Option<String>,
    #[serde(default)]
    pub composer_icon: Option<PluginPathRef>,
    #[serde(default)]
    pub logo: Option<PluginPathRef>,
    #[serde(default)]
    pub logo_dark: Option<PluginPathRef>,
    #[serde(default)]
    pub screenshots: Vec<PluginPathRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum PluginMcpServer {
    Stdio {
        component_key: String,
        bin: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "is_false")]
        requires_exclusive_execution: bool,
    },
    Http {
        component_key: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        oauth_resource: Option<String>,
        #[serde(default)]
        connect_timeout_ms: Option<u64>,
        #[serde(default, skip_serializing_if = "is_false")]
        requires_exclusive_execution: bool,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl PluginMcpServer {
    pub fn component_key(&self) -> &str {
        match self {
            Self::Stdio { component_key, .. } | Self::Http { component_key, .. } => component_key,
        }
    }

    pub fn requires_exclusive_execution(&self) -> bool {
        match self {
            Self::Stdio {
                requires_exclusive_execution,
                ..
            }
            | Self::Http {
                requires_exclusive_execution,
                ..
            } => *requires_exclusive_execution,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginApp {
    pub component_key: String,
    pub manifest: PluginPathRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCommand {
    #[serde(alias = "componentKey")]
    pub component_key: String,
    pub source: PluginPathRef,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, alias = "argumentHint")]
    pub argument_hint: Option<String>,
    #[serde(default, alias = "requiresConfirmation")]
    pub requires_confirmation: bool,
    #[serde(default, alias = "targetAgent")]
    pub target_agent: Option<String>,
    #[serde(default, alias = "allowedTools")]
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginAgent {
    #[serde(alias = "componentKey")]
    pub component_key: String,
    pub source: PluginPathRef,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_plugin_agent_base_agent", alias = "baseAgent")]
    pub base_agent: String,
    #[serde(default, alias = "allowedTools")]
    pub allowed_tools: Vec<String>,
    #[serde(
        default = "default_plugin_agent_max_iterations",
        alias = "maxIterations"
    )]
    pub max_iterations: usize,
}

pub fn default_plugin_agent_base_agent() -> String {
    SystemAgentKey::TaskRunnerRunPhase.as_str().to_string()
}

pub const fn default_plugin_agent_max_iterations() -> usize {
    PLUGIN_AGENT_DEFAULT_MAX_ITERATIONS
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginHook {
    #[serde(alias = "componentKey")]
    pub component_key: String,
    pub source: PluginPathRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginUiContribution {
    pub component_key: String,
    pub source: PluginPathRef,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub assets: Vec<PluginPathRef>,
    #[serde(default)]
    pub bridge_capabilities: Vec<String>,
    #[serde(default)]
    pub artifact_mime_types: Vec<String>,
    #[serde(default)]
    pub runtime: Option<PluginUiRuntime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginUiRuntime {
    LocalHttp {
        bin: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, rename = "healthPath")]
        health_path: Option<String>,
        #[serde(default, rename = "launchTimeoutMs")]
        launch_timeout_ms: Option<u64>,
    },
}

impl PluginUiRuntime {
    pub fn bin(&self) -> &str {
        match self {
            Self::LocalHttp { bin, .. } => bin,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            Self::LocalHttp { args, .. } => args,
        }
    }

    pub fn health_path(&self) -> &str {
        match self {
            Self::LocalHttp { health_path, .. } => health_path
                .as_deref()
                .unwrap_or(PLUGIN_UI_RUNTIME_DEFAULT_HEALTH_PATH),
        }
    }

    pub fn launch_timeout_ms(&self) -> u64 {
        match self {
            Self::LocalHttp {
                launch_timeout_ms, ..
            } => launch_timeout_ms.unwrap_or(PLUGIN_UI_RUNTIME_DEFAULT_LAUNCH_TIMEOUT_MS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDependency {
    pub plugin_id: String,
    #[serde(default)]
    pub version_requirement: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginExecutableDependency {
    pub command: String,
    #[serde(default)]
    pub version_argument: Option<String>,
    #[serde(default)]
    pub version_requirement: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDependencySpec {
    #[serde(default)]
    pub minimum_host_version: Option<String>,
    #[serde(default)]
    pub plugins: Vec<PluginDependency>,
    #[serde(default)]
    pub executables: Vec<PluginExecutableDependency>,
    #[serde(default)]
    pub supported_platforms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPermissionRequirement {
    pub permission: String,
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
}

fn default_required() -> bool {
    true
}

pub(crate) fn component_key_from_path(path: &str, fallback: &str, index: usize) -> String {
    let candidate = path
        .trim_start_matches("./")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(fallback)
        .split('.')
        .next()
        .unwrap_or(fallback);
    let mut key = candidate
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while key.contains("--") {
        key = key.replace("--", "-");
    }
    key = key.trim_matches('-').to_string();
    if key.is_empty() {
        key = fallback.to_string();
    }
    if index > 0 && key == fallback {
        format!("{key}-{}", index + 1)
    } else {
        key
    }
}

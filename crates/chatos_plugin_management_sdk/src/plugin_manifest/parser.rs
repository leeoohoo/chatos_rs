// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

use super::components::{
    component_key_from_path, PluginAgent, PluginApp, PluginAuthor, PluginCommand,
    PluginDependencySpec, PluginHook, PluginInterfaceMetadata, PluginPathRef, PluginRuntimeContext,
    PluginUiContribution, PluginWorkflowContribution,
};
use super::normalized::{PluginManifest, PLUGIN_MANIFEST_SCHEMA_VERSION};
use super::paths::normalize_plugin_relative_path;
use super::{validate_plugin_manifest, PluginManifestError};

mod mcp;
mod permissions;

use mcp::{normalize_mcp_servers, McpServersInput};
use permissions::{normalize_permissions, PermissionInput};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawPluginManifest {
    #[serde(default)]
    schema_version: Option<u32>,
    name: String,
    version: String,
    description: String,
    author: PluginAuthor,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    skills: Option<PathInput>,
    #[serde(default)]
    mcp_servers: Option<McpServersInput>,
    #[serde(default)]
    apps: Option<PathInput>,
    #[serde(default)]
    commands: Option<CommandsInput>,
    #[serde(default)]
    agents: Option<AgentsInput>,
    #[serde(default)]
    hooks: Option<HooksInput>,
    #[serde(default)]
    ui: Option<UiInput>,
    interface: PluginInterfaceMetadata,
    #[serde(default)]
    dependencies: PluginDependencySpec,
    #[serde(default)]
    permissions: Vec<PermissionInput>,
    #[serde(default)]
    runtime_context: Option<PluginRuntimeContext>,
    #[serde(default)]
    workflow: Option<PluginWorkflowContribution>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PathInput {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CommandsInput {
    OnePath(String),
    Paths(Vec<String>),
    OneItem(PluginCommand),
    Items(Vec<PluginCommand>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AgentsInput {
    OnePath(String),
    Paths(Vec<String>),
    OneItem(PluginAgent),
    Items(Vec<PluginAgent>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HooksInput {
    OnePath(String),
    Paths(Vec<String>),
    OneItem(PluginHook),
    Items(Vec<PluginHook>),
}

impl PathInput {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum UiInput {
    OnePath(String),
    Paths(Vec<String>),
    Items(Vec<PluginUiContribution>),
}

pub fn parse_plugin_manifest(raw: &str) -> Result<PluginManifest, PluginManifestError> {
    let raw: RawPluginManifest = serde_json::from_str(raw)?;
    let schema_version = raw.schema_version.unwrap_or(PLUGIN_MANIFEST_SCHEMA_VERSION);
    let manifest = PluginManifest {
        schema_version,
        name: raw.name.trim().to_string(),
        version: raw.version.trim().to_string(),
        description: raw.description.trim().to_string(),
        author: normalize_author(raw.author),
        homepage: normalize_optional(raw.homepage),
        repository: normalize_optional(raw.repository),
        license: normalize_optional(raw.license),
        keywords: normalize_strings(raw.keywords),
        skills: normalize_path_input(raw.skills, "skills")?,
        mcp_servers: normalize_mcp_servers(raw.mcp_servers)?,
        apps: normalize_named_paths(raw.apps, "apps", |component_key, manifest| PluginApp {
            component_key,
            manifest,
        })?,
        commands: normalize_commands(raw.commands)?,
        agents: normalize_agents(raw.agents)?,
        hooks: normalize_hooks(raw.hooks)?,
        ui: normalize_ui(raw.ui)?,
        interface: normalize_interface(raw.interface)?,
        dependencies: raw.dependencies,
        permissions: normalize_permissions(raw.permissions),
        runtime_context: raw.runtime_context.map(normalize_runtime_context),
        workflow: raw.workflow.map(normalize_workflow),
    };
    validate_plugin_manifest(&manifest)?;
    Ok(manifest)
}

fn normalize_workflow(mut workflow: PluginWorkflowContribution) -> PluginWorkflowContribution {
    if let Some(policy) = workflow.task_policy.as_mut() {
        policy.policy_id = policy.policy_id.trim().to_string();
        policy.planning_task.required_builtin_kinds = normalize_strings(std::mem::take(
            &mut policy.planning_task.required_builtin_kinds,
        ));
    }
    workflow
}

fn normalize_runtime_context(mut context: PluginRuntimeContext) -> PluginRuntimeContext {
    context.components = normalize_strings(context.components);
    context.required = normalize_strings(context.required);
    context.optional = normalize_strings(context.optional);
    context
}

fn normalize_author(mut author: PluginAuthor) -> PluginAuthor {
    author.name = author.name.trim().to_string();
    author.email = normalize_optional(author.email);
    author.url = normalize_optional(author.url);
    author
}

fn normalize_interface(
    mut interface: PluginInterfaceMetadata,
) -> Result<PluginInterfaceMetadata, PluginManifestError> {
    interface.display_name = interface.display_name.trim().to_string();
    interface.short_description = interface.short_description.trim().to_string();
    interface.long_description = interface.long_description.trim().to_string();
    interface.developer_name = interface.developer_name.trim().to_string();
    interface.category = interface.category.trim().to_string();
    interface.capabilities = normalize_strings(interface.capabilities);
    interface.website_url = normalize_optional(interface.website_url);
    interface.privacy_policy_url = normalize_optional(interface.privacy_policy_url);
    interface.terms_of_service_url = normalize_optional(interface.terms_of_service_url);
    interface.brand_color = normalize_optional(interface.brand_color);
    interface.default_prompt = interface
        .default_prompt
        .into_iter()
        .map(|value| truncate_chars(value.trim(), 128))
        .filter(|value| !value.is_empty())
        .take(3)
        .collect();
    interface.composer_icon =
        normalize_optional_path(interface.composer_icon, "interface.composerIcon")?;
    interface.logo = normalize_optional_path(interface.logo, "interface.logo")?;
    interface.logo_dark = normalize_optional_path(interface.logo_dark, "interface.logoDark")?;
    interface.screenshots = interface
        .screenshots
        .into_iter()
        .enumerate()
        .map(|(index, value)| normalize_path(value.path, format!("interface.screenshots[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(interface)
}

fn normalize_path_input(
    input: Option<PathInput>,
    field: &str,
) -> Result<Vec<PluginPathRef>, PluginManifestError> {
    input
        .map(PathInput::into_vec)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, value)| normalize_path(value, format!("{field}[{index}]")))
        .collect()
}

fn normalize_named_paths<T, F>(
    input: Option<PathInput>,
    prefix: &str,
    build: F,
) -> Result<Vec<T>, PluginManifestError>
where
    F: Fn(String, PluginPathRef) -> T,
{
    input
        .map(PathInput::into_vec)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let path = normalize_path(value, format!("{prefix}[{index}]"))?;
            let key = component_key_from_path(path.path.as_str(), prefix, index);
            Ok(build(key, path))
        })
        .collect()
}

fn normalize_commands(
    input: Option<CommandsInput>,
) -> Result<Vec<PluginCommand>, PluginManifestError> {
    let values = match input {
        None => return Ok(Vec::new()),
        Some(CommandsInput::OnePath(path)) => vec![PluginCommand {
            component_key: String::new(),
            source: PluginPathRef::new(path),
            description: None,
            argument_hint: None,
            requires_confirmation: false,
            target_agent: None,
            allowed_tools: Vec::new(),
        }],
        Some(CommandsInput::Paths(paths)) => paths
            .into_iter()
            .map(|path| PluginCommand {
                component_key: String::new(),
                source: PluginPathRef::new(path),
                description: None,
                argument_hint: None,
                requires_confirmation: false,
                target_agent: None,
                allowed_tools: Vec::new(),
            })
            .collect(),
        Some(CommandsInput::OneItem(item)) => vec![item],
        Some(CommandsInput::Items(items)) => items,
    };
    values
        .into_iter()
        .enumerate()
        .map(|(index, mut command)| {
            command.source =
                normalize_path(command.source.path, format!("commands[{index}].source"))?;
            command.component_key = if command.component_key.trim().is_empty() {
                component_key_from_path(command.source.path.as_str(), "commands", index)
            } else {
                command.component_key.trim().to_string()
            };
            command.description = normalize_optional(command.description)
                .map(|value| truncate_chars(value.as_str(), 4096));
            command.argument_hint = normalize_optional(command.argument_hint)
                .map(|value| truncate_chars(value.as_str(), 1024));
            command.target_agent = normalize_optional(command.target_agent);
            command.allowed_tools = command
                .allowed_tools
                .into_iter()
                .map(|value| value.trim().to_string())
                .collect();
            command.allowed_tools.sort();
            Ok(command)
        })
        .collect()
}

fn normalize_agents(input: Option<AgentsInput>) -> Result<Vec<PluginAgent>, PluginManifestError> {
    let values = match input {
        None => return Ok(Vec::new()),
        Some(AgentsInput::OnePath(path)) => vec![PluginAgent {
            component_key: String::new(),
            source: PluginPathRef::new(path),
            description: None,
            base_agent: super::components::default_plugin_agent_base_agent(),
            allowed_tools: Vec::new(),
            max_iterations: super::components::default_plugin_agent_max_iterations(),
        }],
        Some(AgentsInput::Paths(paths)) => paths
            .into_iter()
            .map(|path| PluginAgent {
                component_key: String::new(),
                source: PluginPathRef::new(path),
                description: None,
                base_agent: super::components::default_plugin_agent_base_agent(),
                allowed_tools: Vec::new(),
                max_iterations: super::components::default_plugin_agent_max_iterations(),
            })
            .collect(),
        Some(AgentsInput::OneItem(item)) => vec![item],
        Some(AgentsInput::Items(items)) => items,
    };
    values
        .into_iter()
        .enumerate()
        .map(|(index, mut agent)| {
            agent.source = normalize_path(agent.source.path, format!("agents[{index}].source"))?;
            agent.component_key = if agent.component_key.trim().is_empty() {
                component_key_from_path(agent.source.path.as_str(), "agents", index)
            } else {
                agent.component_key.trim().to_string()
            };
            agent.description = normalize_optional(agent.description)
                .map(|value| truncate_chars(value.as_str(), 4096));
            agent.base_agent = agent.base_agent.trim().to_string();
            agent.allowed_tools = agent
                .allowed_tools
                .into_iter()
                .map(|value| value.trim().to_string())
                .collect();
            agent.allowed_tools.sort();
            Ok(agent)
        })
        .collect()
}

fn normalize_hooks(input: Option<HooksInput>) -> Result<Vec<PluginHook>, PluginManifestError> {
    let values = match input {
        None => return Ok(Vec::new()),
        Some(HooksInput::OnePath(path)) => vec![PluginHook {
            component_key: String::new(),
            source: PluginPathRef::new(path),
        }],
        Some(HooksInput::Paths(paths)) => paths
            .into_iter()
            .map(|path| PluginHook {
                component_key: String::new(),
                source: PluginPathRef::new(path),
            })
            .collect(),
        Some(HooksInput::OneItem(item)) => vec![item],
        Some(HooksInput::Items(items)) => items,
    };
    values
        .into_iter()
        .enumerate()
        .map(|(index, mut hook)| {
            hook.source = normalize_path(hook.source.path, format!("hooks[{index}].source"))?;
            hook.component_key = if hook.component_key.trim().is_empty() {
                component_key_from_path(hook.source.path.as_str(), "hooks", index)
            } else {
                hook.component_key.trim().to_string()
            };
            Ok(hook)
        })
        .collect()
}

fn normalize_ui(input: Option<UiInput>) -> Result<Vec<PluginUiContribution>, PluginManifestError> {
    match input {
        None => Ok(Vec::new()),
        Some(UiInput::OnePath(path)) => normalize_ui_paths(vec![path]),
        Some(UiInput::Paths(paths)) => normalize_ui_paths(paths),
        Some(UiInput::Items(items)) => items
            .into_iter()
            .enumerate()
            .map(|(index, mut item)| {
                item.component_key = item.component_key.trim().to_string();
                item.source = normalize_path(item.source.path, format!("ui[{index}].source"))?;
                item.title =
                    normalize_optional(item.title).map(|value| truncate_chars(value.as_str(), 128));
                item.surface =
                    normalize_optional(item.surface).map(|value| value.to_ascii_lowercase());
                item.assets = item
                    .assets
                    .into_iter()
                    .enumerate()
                    .map(|(asset_index, asset)| {
                        normalize_path(asset.path, format!("ui[{index}].assets[{asset_index}]"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                item.assets
                    .sort_by(|left, right| left.path.cmp(&right.path));
                item.bridge_capabilities = normalize_strings(item.bridge_capabilities);
                item.artifact_mime_types = normalize_strings(
                    item.artifact_mime_types
                        .into_iter()
                        .map(|value| value.to_ascii_lowercase())
                        .collect(),
                );
                if let Some(runtime) = &mut item.runtime {
                    match runtime {
                        super::components::PluginUiRuntime::LocalHttp {
                            bin, health_path, ..
                        } => {
                            *bin = bin.trim().to_string();
                            *health_path = normalize_optional(health_path.take());
                        }
                    }
                }
                Ok(item)
            })
            .collect(),
    }
}

fn normalize_ui_paths(
    paths: Vec<String>,
) -> Result<Vec<PluginUiContribution>, PluginManifestError> {
    paths
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let source = normalize_path(value, format!("ui[{index}]"))?;
            Ok(PluginUiContribution {
                component_key: component_key_from_path(source.path.as_str(), "ui", index),
                source,
                title: None,
                surface: None,
                assets: Vec::new(),
                bridge_capabilities: Vec::new(),
                artifact_mime_types: Vec::new(),
                runtime: None,
            })
        })
        .collect()
}

fn normalize_path(value: String, field: String) -> Result<PluginPathRef, PluginManifestError> {
    normalize_plugin_relative_path(value.as_str())
        .map(PluginPathRef::new)
        .map_err(|message| PluginManifestError::InvalidField { field, message })
}

fn normalize_optional_path(
    value: Option<PluginPathRef>,
    field: &str,
) -> Result<Option<PluginPathRef>, PluginManifestError> {
    value
        .map(|value| normalize_path(value.path, field.to_string()))
        .transpose()
}

fn required_optional(value: Option<String>, field: String) -> Result<String, PluginManifestError> {
    normalize_optional(value).ok_or_else(|| PluginManifestError::InvalidField {
        field,
        message: "field is required".to_string(),
    })
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_strings(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn invalid_field<T>(field: String, message: impl Into<String>) -> Result<T, PluginManifestError> {
    Err(PluginManifestError::InvalidField {
        field,
        message: message.into(),
    })
}

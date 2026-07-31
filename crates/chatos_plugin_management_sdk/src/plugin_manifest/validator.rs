// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use semver::{Version, VersionReq};

use super::components::{
    component_key_from_path, PluginMcpServer, PluginPathRef, PLUGIN_AGENT_MAX_ITERATIONS,
    PLUGIN_COMMAND_MAX_ALLOWED_TOOLS, PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE, PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ,
    PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES, PLUGIN_UI_MAX_ASSETS, PLUGIN_UI_MAX_BRIDGE_CAPABILITIES,
    PLUGIN_UI_SURFACE_ARTIFACT_VIEWER, PLUGIN_UI_SURFACE_DETAIL_PANEL,
    PLUGIN_UI_SURFACE_MESSAGE_PANEL, PLUGIN_UI_SURFACE_WORKBENCH,
};
use super::normalized::{
    PluginExecutionHost, PluginManifest, PLUGIN_MANIFEST_SCHEMA_VERSION_V1,
    PLUGIN_MANIFEST_SCHEMA_VERSION_V2,
};
use super::paths::normalize_plugin_relative_path;
use super::validation_support::{
    issue, required_text, validate_brand_color, validate_mcp_http_url, validate_optional_email,
    validate_optional_https_url, validate_stdio_environment,
};
use super::PluginManifestValidationIssue;
use crate::SystemAgentKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifestValidationError {
    pub issues: Vec<PluginManifestValidationIssue>,
}

impl fmt::Display for PluginManifestValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let summary = self
            .issues
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        write!(formatter, "plugin manifest validation failed: {summary}")
    }
}

impl std::error::Error for PluginManifestValidationError {}

pub fn validate_plugin_manifest(
    manifest: &PluginManifest,
) -> Result<(), PluginManifestValidationError> {
    let mut issues = Vec::new();

    if ![
        PLUGIN_MANIFEST_SCHEMA_VERSION_V1,
        PLUGIN_MANIFEST_SCHEMA_VERSION_V2,
    ]
    .contains(&manifest.schema_version)
    {
        issue(
            &mut issues,
            "schemaVersion",
            format!(
                "unsupported schema version {}; expected {} or {}",
                manifest.schema_version,
                PLUGIN_MANIFEST_SCHEMA_VERSION_V1,
                PLUGIN_MANIFEST_SCHEMA_VERSION_V2
            ),
        );
    }
    validate_plugin_name(manifest.name.as_str(), &mut issues);
    if Version::parse(manifest.version.as_str()).is_err() {
        issue(&mut issues, "version", "version must use strict semver");
    }
    required_text(&mut issues, "description", manifest.description.as_str());
    required_text(&mut issues, "author.name", manifest.author.name.as_str());
    validate_optional_email(manifest.author.email.as_deref(), &mut issues);
    validate_optional_https_url("author.url", manifest.author.url.as_deref(), &mut issues);
    validate_optional_https_url("homepage", manifest.homepage.as_deref(), &mut issues);
    validate_optional_https_url("repository", manifest.repository.as_deref(), &mut issues);

    required_text(
        &mut issues,
        "interface.displayName",
        manifest.interface.display_name.as_str(),
    );
    required_text(
        &mut issues,
        "interface.shortDescription",
        manifest.interface.short_description.as_str(),
    );
    required_text(
        &mut issues,
        "interface.longDescription",
        manifest.interface.long_description.as_str(),
    );
    required_text(
        &mut issues,
        "interface.developerName",
        manifest.interface.developer_name.as_str(),
    );
    required_text(
        &mut issues,
        "interface.category",
        manifest.interface.category.as_str(),
    );
    validate_optional_https_url(
        "interface.websiteURL",
        manifest.interface.website_url.as_deref(),
        &mut issues,
    );
    validate_optional_https_url(
        "interface.privacyPolicyURL",
        manifest.interface.privacy_policy_url.as_deref(),
        &mut issues,
    );
    validate_optional_https_url(
        "interface.termsOfServiceURL",
        manifest.interface.terms_of_service_url.as_deref(),
        &mut issues,
    );
    validate_brand_color(manifest.interface.brand_color.as_deref(), &mut issues);
    validate_interface_assets(manifest, &mut issues);

    let mut component_keys = HashSet::new();
    for (index, path) in manifest.skills.iter().enumerate() {
        validate_path(format!("skills[{index}]"), path, &mut issues);
        let key = component_key_from_path(path.path.as_str(), "skills", index);
        validate_component_key(
            format!("skills[{index}].component_key"),
            key.as_str(),
            &mut component_keys,
            &mut issues,
        );
    }

    for (index, server) in manifest.mcp_servers.iter().enumerate() {
        validate_component_key(
            format!("mcpServers[{index}].component_key"),
            server.component_key(),
            &mut component_keys,
            &mut issues,
        );
        match server {
            PluginMcpServer::ConfigFile { path, .. } => {
                validate_path(format!("mcpServers[{index}].path"), path, &mut issues)
            }
            PluginMcpServer::Stdio {
                command,
                args,
                env,
                cwd,
                ..
            } => {
                validate_stdio_command(index, command, args, &mut issues);
                validate_stdio_environment(index, env, &mut issues);
                if let Some(cwd) = cwd {
                    validate_path(format!("mcpServers[{index}].cwd"), cwd, &mut issues);
                }
            }
            PluginMcpServer::Http { url, .. } => {
                validate_mcp_http_url(format!("mcpServers[{index}].url"), url, &mut issues)
            }
        }
    }

    for (index, app) in manifest.apps.iter().enumerate() {
        validate_component_key(
            format!("apps[{index}].component_key"),
            app.component_key.as_str(),
            &mut component_keys,
            &mut issues,
        );
        validate_path(
            format!("apps[{index}].manifest"),
            &app.manifest,
            &mut issues,
        );
    }
    for (index, command) in manifest.commands.iter().enumerate() {
        validate_component_key(
            format!("commands[{index}].component_key"),
            command.component_key.as_str(),
            &mut component_keys,
            &mut issues,
        );
        validate_path(
            format!("commands[{index}].source"),
            &command.source,
            &mut issues,
        );
        if command
            .description
            .as_ref()
            .is_some_and(|value| value.len() > 4096)
        {
            issue(
                &mut issues,
                format!("commands[{index}].description").as_str(),
                "description exceeds 4096 bytes",
            );
        }
        if command
            .argument_hint
            .as_ref()
            .is_some_and(|value| value.len() > 1024)
        {
            issue(
                &mut issues,
                format!("commands[{index}].argument_hint").as_str(),
                "argument hint exceeds 1024 bytes",
            );
        }
        if let Some(target_agent) = command.target_agent.as_deref() {
            if ![
                SystemAgentKey::TaskRunnerPlanPhase.as_str(),
                SystemAgentKey::TaskRunnerLocalPlanPhase.as_str(),
                SystemAgentKey::TaskRunnerRunPhase.as_str(),
                SystemAgentKey::TaskRunnerLocalRunPhase.as_str(),
            ]
            .contains(&target_agent)
            {
                issue(
                    &mut issues,
                    format!("commands[{index}].target_agent").as_str(),
                    "target agent must be task_runner_plan_phase, task_runner_local_plan_phase, task_runner_run_phase, or task_runner_local_run_phase",
                );
            }
        }
        if command.allowed_tools.len() > PLUGIN_COMMAND_MAX_ALLOWED_TOOLS {
            issue(
                &mut issues,
                format!("commands[{index}].allowed_tools").as_str(),
                format!(
                    "allowed tools must contain at most {PLUGIN_COMMAND_MAX_ALLOWED_TOOLS} items"
                ),
            );
        }
        let mut allowed_tools = HashSet::new();
        for (tool_index, tool_name) in command.allowed_tools.iter().enumerate() {
            let valid = !tool_name.is_empty()
                && tool_name.len() <= PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES
                && tool_name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
            if !valid {
                issue(
                    &mut issues,
                    format!("commands[{index}].allowed_tools[{tool_index}]").as_str(),
                    "allowed tool must be a 1-256 byte canonical public tool name",
                );
            } else if !allowed_tools.insert(tool_name.as_str()) {
                issue(
                    &mut issues,
                    format!("commands[{index}].allowed_tools[{tool_index}]").as_str(),
                    "duplicate allowed tool",
                );
            }
        }
    }
    for (index, agent) in manifest.agents.iter().enumerate() {
        validate_component_key(
            format!("agents[{index}].component_key"),
            agent.component_key.as_str(),
            &mut component_keys,
            &mut issues,
        );
        validate_path(
            format!("agents[{index}].source"),
            &agent.source,
            &mut issues,
        );
        if agent
            .description
            .as_ref()
            .is_some_and(|value| value.len() > 4096)
        {
            issue(
                &mut issues,
                format!("agents[{index}].description").as_str(),
                "description exceeds 4096 bytes",
            );
        }
        if ![
            SystemAgentKey::TaskRunnerPlanPhase.as_str(),
            SystemAgentKey::TaskRunnerLocalPlanPhase.as_str(),
            SystemAgentKey::TaskRunnerRunPhase.as_str(),
            SystemAgentKey::TaskRunnerLocalRunPhase.as_str(),
        ]
        .contains(&agent.base_agent.as_str())
        {
            issue(
                &mut issues,
                format!("agents[{index}].base_agent").as_str(),
                "base agent must be task_runner_plan_phase, task_runner_local_plan_phase, task_runner_run_phase, or task_runner_local_run_phase",
            );
        }
        validate_allowed_tools(
            format!("agents[{index}].allowed_tools").as_str(),
            &agent.allowed_tools,
            &mut issues,
        );
        if !(1..=PLUGIN_AGENT_MAX_ITERATIONS).contains(&agent.max_iterations) {
            issue(
                &mut issues,
                format!("agents[{index}].max_iterations").as_str(),
                format!("max iterations must be between 1 and {PLUGIN_AGENT_MAX_ITERATIONS}"),
            );
        }
    }
    for (index, hook) in manifest.hooks.iter().enumerate() {
        validate_component_key(
            format!("hooks[{index}].component_key"),
            hook.component_key.as_str(),
            &mut component_keys,
            &mut issues,
        );
        validate_path(format!("hooks[{index}].source"), &hook.source, &mut issues);
    }
    for (index, ui) in manifest.ui.iter().enumerate() {
        validate_component_key(
            format!("ui[{index}].component_key"),
            ui.component_key.as_str(),
            &mut component_keys,
            &mut issues,
        );
        validate_path(format!("ui[{index}].source"), &ui.source, &mut issues);
        validate_ui_contribution(index, ui, &mut issues);
    }

    validate_dependencies(manifest, &mut issues);
    validate_permissions(manifest, &component_keys, &mut issues);
    validate_execution_policy(manifest, &component_keys, &mut issues);

    let component_count = manifest.skills.len()
        + manifest.mcp_servers.len()
        + manifest.apps.len()
        + manifest.commands.len()
        + manifest.agents.len()
        + manifest.hooks.len()
        + manifest.ui.len();
    if component_count == 0 {
        issue(
            &mut issues,
            "components",
            "plugin must declare at least one component",
        );
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(PluginManifestValidationError { issues })
    }
}

fn validate_execution_policy(
    manifest: &PluginManifest,
    component_keys: &HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if manifest.schema_version == PLUGIN_MANIFEST_SCHEMA_VERSION_V1 {
        if !manifest.execution.is_implicit_v1() {
            issue(
                issues,
                "execution",
                "schemaVersion 1 must not declare execution",
            );
        }
        return;
    }

    let mut kinds = BTreeMap::new();
    for (index, skill) in manifest.skills.iter().enumerate() {
        kinds.insert(
            component_key_from_path(skill.path.as_str(), "skills", index),
            "skill",
        );
    }
    for component in &manifest.mcp_servers {
        kinds.insert(component.component_key().to_string(), "mcp_server");
    }
    for component in &manifest.apps {
        kinds.insert(component.component_key.clone(), "connected_app");
    }
    for component in &manifest.commands {
        kinds.insert(component.component_key.clone(), "command");
    }
    for component in &manifest.agents {
        kinds.insert(component.component_key.clone(), "agent");
    }
    for component in &manifest.hooks {
        kinds.insert(component.component_key.clone(), "hook_set");
    }
    for component in &manifest.ui {
        kinds.insert(component.component_key.clone(), "ui_contribution");
    }

    for component_key in manifest.execution.component_hosts.keys() {
        if !component_keys.contains(component_key) {
            issue(
                issues,
                "execution.componentHosts",
                format!("unknown component key {component_key}"),
            );
        }
    }

    for (component_key, kind) in &kinds {
        let host = manifest.execution.host_for(component_key);
        if host != PluginExecutionHost::Local
            && !matches!(*kind, "skill" | "command" | "agent" | "mcp_server")
        {
            issue(
                issues,
                "execution",
                format!("{kind} component {component_key} must use local execution"),
            );
        }
    }

    for (index, permission) in manifest.permissions.iter().enumerate() {
        let cloud_targets = if permission.components.is_empty() {
            kinds
                .keys()
                .filter(|key| manifest.execution.host_for(key) != PluginExecutionHost::Local)
                .map(String::as_str)
                .collect::<Vec<_>>()
        } else {
            permission
                .components
                .iter()
                .filter(|key| manifest.execution.host_for(key) != PluginExecutionHost::Local)
                .map(String::as_str)
                .collect::<Vec<_>>()
        };
        let targets_non_mcp_cloud_component = cloud_targets
            .iter()
            .any(|key| kinds.get(*key).copied() != Some("mcp_server"));
        if targets_non_mcp_cloud_component {
            issue(
                issues,
                format!("permissions[{index}]").as_str(),
                "cloud and portable prompt components must not request runtime permissions",
            );
        }
    }
}

fn validate_ui_contribution(
    index: usize,
    ui: &super::components::PluginUiContribution,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if !ui.source.path.starts_with("./ui/")
        || !ui.source.path.to_ascii_lowercase().ends_with(".html")
    {
        issue(
            issues,
            format!("ui[{index}].source").as_str(),
            "Plugin UI entrypoint must be an HTML file under ./ui/",
        );
    }
    if ui
        .title
        .as_ref()
        .is_some_and(|title| title.is_empty() || title.len() > 512)
    {
        issue(
            issues,
            format!("ui[{index}].title").as_str(),
            "Plugin UI title must contain at most 128 Unicode characters",
        );
    }
    if let Some(surface) = ui.surface.as_deref() {
        if ![
            PLUGIN_UI_SURFACE_DETAIL_PANEL,
            PLUGIN_UI_SURFACE_MESSAGE_PANEL,
            PLUGIN_UI_SURFACE_WORKBENCH,
            PLUGIN_UI_SURFACE_ARTIFACT_VIEWER,
        ]
        .contains(&surface)
        {
            issue(
                issues,
                format!("ui[{index}].surface").as_str(),
                "Plugin UI surface must be detail_panel, message_panel, workbench, or artifact_viewer",
            );
        }
    }
    if ui.assets.len() > PLUGIN_UI_MAX_ASSETS {
        issue(
            issues,
            format!("ui[{index}].assets").as_str(),
            format!("Plugin UI assets must contain at most {PLUGIN_UI_MAX_ASSETS} items"),
        );
    }
    let mut assets = HashSet::new();
    for (asset_index, asset) in ui.assets.iter().enumerate() {
        let field = format!("ui[{index}].assets[{asset_index}]");
        validate_path(field.clone(), asset, issues);
        let extension = asset
            .path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        let safe_extension = extension.as_deref().is_some_and(|extension| {
            matches!(
                extension,
                "js" | "mjs"
                    | "css"
                    | "json"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "woff"
                    | "woff2"
            )
        });
        if !asset.path.starts_with("./ui/") || !safe_extension {
            issue(
                issues,
                field.as_str(),
                "Plugin UI asset must use a supported static extension under ./ui/",
            );
        } else if asset.path == ui.source.path || !assets.insert(asset.path.as_str()) {
            issue(
                issues,
                field.as_str(),
                "Plugin UI assets must be unique and cannot repeat the entrypoint",
            );
        }
    }
    if ui.bridge_capabilities.len() > PLUGIN_UI_MAX_BRIDGE_CAPABILITIES {
        issue(
            issues,
            format!("ui[{index}].bridgeCapabilities").as_str(),
            format!(
                "Plugin UI bridge capabilities must contain at most {PLUGIN_UI_MAX_BRIDGE_CAPABILITIES} items"
            ),
        );
    }
    let allowed_capabilities = [
        PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
    ];
    let mut capabilities = HashSet::new();
    for (capability_index, capability) in ui.bridge_capabilities.iter().enumerate() {
        if !allowed_capabilities.contains(&capability.as_str()) {
            issue(
                issues,
                format!("ui[{index}].bridgeCapabilities[{capability_index}]").as_str(),
                "Plugin UI bridge capability is not supported",
            );
        } else if !capabilities.insert(capability.as_str()) {
            issue(
                issues,
                format!("ui[{index}].bridgeCapabilities[{capability_index}]").as_str(),
                "duplicate Plugin UI bridge capability",
            );
        }
    }
    if ui.artifact_mime_types.len() > PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES {
        issue(
            issues,
            format!("ui[{index}].artifactMimeTypes").as_str(),
            format!(
                "Plugin UI artifact MIME types must contain at most {PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES} items"
            ),
        );
    }
    let mut mime_types = HashSet::new();
    for (mime_index, mime_type) in ui.artifact_mime_types.iter().enumerate() {
        let valid = mime_type.len() <= 128
            && mime_type.split_once('/').is_some_and(|(kind, subtype)| {
                !kind.is_empty()
                    && !subtype.is_empty()
                    && kind.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                    && subtype.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'-' | b'+' | b'.')
                    })
            });
        if !valid {
            issue(
                issues,
                format!("ui[{index}].artifactMimeTypes[{mime_index}]").as_str(),
                "Plugin UI artifact MIME type is invalid",
            );
        } else if !mime_types.insert(mime_type.as_str()) {
            issue(
                issues,
                format!("ui[{index}].artifactMimeTypes[{mime_index}]").as_str(),
                "duplicate Plugin UI artifact MIME type",
            );
        }
    }
}

fn validate_allowed_tools(
    field: &str,
    values: &[String],
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if values.len() > PLUGIN_COMMAND_MAX_ALLOWED_TOOLS {
        issue(
            issues,
            field,
            format!("allowed tools must contain at most {PLUGIN_COMMAND_MAX_ALLOWED_TOOLS} items"),
        );
    }
    let mut allowed_tools = HashSet::new();
    for (tool_index, tool_name) in values.iter().enumerate() {
        let valid = !tool_name.is_empty()
            && tool_name.len() <= PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES
            && tool_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
        if !valid {
            issue(
                issues,
                format!("{field}[{tool_index}]").as_str(),
                "allowed tool must be a 1-256 byte canonical public tool name",
            );
        } else if !allowed_tools.insert(tool_name.as_str()) {
            issue(
                issues,
                format!("{field}[{tool_index}]").as_str(),
                "duplicate allowed tool",
            );
        }
    }
}

fn validate_plugin_name(value: &str, issues: &mut Vec<PluginManifestValidationIssue>) {
    let valid = !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        issue(
            issues,
            "name",
            "name must be 1-64 characters of lower-case kebab-case",
        );
    }
}

fn validate_component_key(
    field: String,
    value: &str,
    keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid {
        issue(
            issues,
            field.as_str(),
            "component key contains unsupported characters",
        );
    } else if !keys.insert(value.to_string()) {
        issue(issues, field.as_str(), "duplicate component key");
    }
}

fn validate_path(
    field: String,
    value: &PluginPathRef,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    match normalize_plugin_relative_path(value.path.as_str()) {
        Ok(normalized) if normalized == value.path => {}
        Ok(_) => issue(issues, field.as_str(), "path is not normalized"),
        Err(message) => issue(issues, field.as_str(), message),
    }
}

fn validate_interface_assets(
    manifest: &PluginManifest,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (field, path) in [
        (
            "interface.composerIcon",
            manifest.interface.composer_icon.as_ref(),
        ),
        ("interface.logo", manifest.interface.logo.as_ref()),
        ("interface.logoDark", manifest.interface.logo_dark.as_ref()),
    ] {
        if let Some(path) = path {
            validate_path(field.to_string(), path, issues);
            if !path.path.starts_with("./assets/") {
                issue(issues, field, "asset must be stored under ./assets/");
            }
        }
    }
    for (index, path) in manifest.interface.screenshots.iter().enumerate() {
        let field = format!("interface.screenshots[{index}]");
        validate_path(field.clone(), path, issues);
        if !path.path.starts_with("./assets/") || !path.path.to_ascii_lowercase().ends_with(".png")
        {
            issue(
                issues,
                field.as_str(),
                "screenshot must be a PNG under ./assets/",
            );
        }
    }
}

fn validate_dependencies(
    manifest: &PluginManifest,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if let Some(version) = manifest.dependencies.minimum_host_version.as_deref() {
        if VersionReq::parse(version).is_err() && Version::parse(version).is_err() {
            issue(
                issues,
                "dependencies.minimumHostVersion",
                "minimum host version must be semver or a semver requirement",
            );
        }
    }
    for (index, dependency) in manifest.dependencies.plugins.iter().enumerate() {
        required_text(
            issues,
            format!("dependencies.plugins[{index}].pluginId").as_str(),
            dependency.plugin_id.as_str(),
        );
        if let Some(requirement) = dependency.version_requirement.as_deref() {
            if VersionReq::parse(requirement).is_err() {
                issue(
                    issues,
                    format!("dependencies.plugins[{index}].versionRequirement").as_str(),
                    "version requirement must use semver syntax",
                );
            }
        }
    }
}

fn validate_permissions(
    manifest: &PluginManifest,
    component_keys: &HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let mut permissions = HashSet::new();
    for (index, requirement) in manifest.permissions.iter().enumerate() {
        let field = format!("permissions[{index}].permission");
        let permission = requirement.permission.as_str();
        let valid = !permission.is_empty()
            && permission.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b':' | b'-' | b'_' | b'*')
            });
        if !valid {
            issue(
                issues,
                field.as_str(),
                "permission must use a lower-case capability identifier",
            );
        } else if !permissions.insert(permission.to_string()) {
            issue(issues, field.as_str(), "duplicate permission declaration");
        }
        for component in &requirement.components {
            if !component_keys.contains(component) {
                issue(
                    issues,
                    format!("permissions[{index}].components").as_str(),
                    format!("unknown component key {component}"),
                );
            }
        }
    }
}

fn validate_stdio_command(
    index: usize,
    command: &str,
    args: &[String],
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let field = format!("mcpServers[{index}].command");
    if command.trim().is_empty() {
        issue(issues, field.as_str(), "command cannot be empty");
        return;
    }
    if command.contains('/') {
        if let Err(message) = normalize_plugin_relative_path(command) {
            issue(issues, field.as_str(), message);
        }
    } else if !command
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        issue(
            issues,
            field.as_str(),
            "command must be a signed relative path or reviewed command identifier",
        );
    }

    let shell = command
        .rsplit('/')
        .next()
        .unwrap_or(command)
        .to_ascii_lowercase();
    let has_shell_eval = match shell.as_str() {
        "sh" | "bash" | "zsh" => args.iter().any(|arg| arg == "-c"),
        "cmd" | "cmd.exe" => args.iter().any(|arg| arg.eq_ignore_ascii_case("/c")),
        "powershell" | "powershell.exe" | "pwsh" => args
            .iter()
            .any(|arg| matches!(arg.to_ascii_lowercase().as_str(), "-command" | "-c")),
        _ => false,
    };
    if has_shell_eval {
        issue(
            issues,
            field.as_str(),
            "generic shell evaluation is not allowed for plugin MCP entrypoints",
        );
    }
}

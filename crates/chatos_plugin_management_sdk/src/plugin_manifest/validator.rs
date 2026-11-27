// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::fmt;

use semver::{Version, VersionReq};

use super::components::{
    component_key_from_path, PluginMcpServer, PluginPathRef, PluginRuntimeContext,
    PluginRuntimeContextScope, PluginRuntimeContextStorageIsolation, PLUGIN_AGENT_MAX_ITERATIONS,
    PLUGIN_COMMAND_MAX_ALLOWED_TOOLS, PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES,
    PLUGIN_RUNTIME_CONTEXT_FIELD_PROJECT_ID, PLUGIN_RUNTIME_CONTEXT_FIELD_WORKSPACE_ID,
    PLUGIN_RUNTIME_CONTEXT_FIELD_WORKSPACE_ROOT, PLUGIN_RUNTIME_CONTEXT_MAX_COMPONENTS,
    PLUGIN_RUNTIME_CONTEXT_MAX_FIELDS, PLUGIN_TASK_POLICY_MAX_BUILTIN_KINDS,
};
use super::normalized::{PluginManifest, PLUGIN_MANIFEST_SCHEMA_VERSION};
use super::paths::normalize_plugin_relative_path;
use super::validation_support::{
    issue, required_text, validate_brand_color, validate_mcp_http_url, validate_optional_email,
    validate_optional_https_url, validate_stdio_environment,
};
use super::PluginManifestValidationIssue;
use crate::SystemAgentKey;

mod stdio;
use stdio::*;
mod rules;
use rules::*;
mod ui;
use ui::validate_ui_contribution;

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

    validate_manifest_metadata(manifest, &mut issues);
    let mut component_keys = HashSet::new();
    validate_skills(manifest, &mut component_keys, &mut issues);
    validate_mcp_servers(manifest, &mut component_keys, &mut issues);
    validate_apps(manifest, &mut component_keys, &mut issues);
    validate_commands(manifest, &mut component_keys, &mut issues);
    validate_agents(manifest, &mut component_keys, &mut issues);
    validate_hooks(manifest, &mut component_keys, &mut issues);
    validate_ui(manifest, &mut component_keys, &mut issues);

    validate_dependencies(manifest, &mut issues);
    validate_permissions(manifest, &component_keys, &mut issues);
    validate_runtime_context(manifest, &component_keys, &mut issues);
    validate_workflow(manifest, &mut issues);
    validate_mcp_runtime_permissions(manifest, &mut issues);
    validate_ui_runtime_permissions(manifest, &mut issues);

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

fn validate_workflow(manifest: &PluginManifest, issues: &mut Vec<PluginManifestValidationIssue>) {
    let Some(policy) = manifest
        .workflow
        .as_ref()
        .and_then(|workflow| workflow.task_policy.as_ref())
    else {
        return;
    };
    if !valid_identifier(policy.policy_id.as_str()) {
        issue(
            issues,
            "workflow.taskPolicy.policyId",
            "policy id must be a canonical identifier",
        );
    }
    let kinds = &policy.planning_task.required_builtin_kinds;
    if kinds.is_empty() || kinds.len() > PLUGIN_TASK_POLICY_MAX_BUILTIN_KINDS {
        issue(
            issues,
            "workflow.taskPolicy.planningTask.requiredBuiltinKinds",
            format!("must contain between 1 and {PLUGIN_TASK_POLICY_MAX_BUILTIN_KINDS} entries"),
        );
    }
    let mut seen = HashSet::new();
    for (index, kind) in kinds.iter().enumerate() {
        if !valid_identifier(kind) {
            issue(
                issues,
                format!("workflow.taskPolicy.planningTask.requiredBuiltinKinds[{index}]").as_str(),
                "builtin kind must be a canonical identifier",
            );
        } else if !seen.insert(kind) {
            issue(
                issues,
                "workflow.taskPolicy.planningTask.requiredBuiltinKinds",
                "builtin kinds must be unique",
            );
        }
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn validate_manifest_metadata(
    manifest: &PluginManifest,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if manifest.schema_version != PLUGIN_MANIFEST_SCHEMA_VERSION {
        issue(
            issues,
            "schemaVersion",
            format!(
                "unsupported schema version {}; expected {}",
                manifest.schema_version, PLUGIN_MANIFEST_SCHEMA_VERSION
            ),
        );
    }
    validate_plugin_name(manifest.name.as_str(), issues);
    if Version::parse(manifest.version.as_str()).is_err() {
        issue(issues, "version", "version must use strict semver");
    }
    required_text(issues, "description", manifest.description.as_str());
    required_text(issues, "author.name", manifest.author.name.as_str());
    validate_optional_email(manifest.author.email.as_deref(), issues);
    validate_optional_https_url("author.url", manifest.author.url.as_deref(), issues);
    validate_optional_https_url("homepage", manifest.homepage.as_deref(), issues);
    validate_optional_https_url("repository", manifest.repository.as_deref(), issues);

    required_text(
        issues,
        "interface.displayName",
        manifest.interface.display_name.as_str(),
    );
    required_text(
        issues,
        "interface.shortDescription",
        manifest.interface.short_description.as_str(),
    );
    required_text(
        issues,
        "interface.longDescription",
        manifest.interface.long_description.as_str(),
    );
    required_text(
        issues,
        "interface.developerName",
        manifest.interface.developer_name.as_str(),
    );
    required_text(
        issues,
        "interface.category",
        manifest.interface.category.as_str(),
    );
    validate_optional_https_url(
        "interface.websiteURL",
        manifest.interface.website_url.as_deref(),
        issues,
    );
    validate_optional_https_url(
        "interface.privacyPolicyURL",
        manifest.interface.privacy_policy_url.as_deref(),
        issues,
    );
    validate_optional_https_url(
        "interface.termsOfServiceURL",
        manifest.interface.terms_of_service_url.as_deref(),
        issues,
    );
    validate_brand_color(manifest.interface.brand_color.as_deref(), issues);
    validate_interface_assets(manifest, issues);
}

fn validate_skills(
    manifest: &PluginManifest,
    component_keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, path) in manifest.skills.iter().enumerate() {
        validate_path(format!("skills[{index}]"), path, issues);
        let key = component_key_from_path(path.path.as_str(), "skills", index);
        validate_component_key(
            format!("skills[{index}].component_key"),
            key.as_str(),
            component_keys,
            issues,
        );
    }
}

fn validate_mcp_servers(
    manifest: &PluginManifest,
    component_keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, server) in manifest.mcp_servers.iter().enumerate() {
        validate_component_key(
            format!("mcpServers[{index}].component_key"),
            server.component_key(),
            component_keys,
            issues,
        );
        match server {
            PluginMcpServer::Stdio { bin, args, env, .. } => {
                validate_npm_bin(index, bin, args, issues);
                validate_stdio_environment(index, env, issues);
            }
            PluginMcpServer::Http { url, .. } => {
                validate_mcp_http_url(format!("mcpServers[{index}].url"), url, issues)
            }
        }
    }
}

fn validate_apps(
    manifest: &PluginManifest,
    component_keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, app) in manifest.apps.iter().enumerate() {
        validate_component_key(
            format!("apps[{index}].component_key"),
            app.component_key.as_str(),
            component_keys,
            issues,
        );
        validate_path(format!("apps[{index}].manifest"), &app.manifest, issues);
    }
}

fn validate_commands(
    manifest: &PluginManifest,
    component_keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, command) in manifest.commands.iter().enumerate() {
        validate_component_key(
            format!("commands[{index}].component_key"),
            command.component_key.as_str(),
            component_keys,
            issues,
        );
        validate_path(format!("commands[{index}].source"), &command.source, issues);
        validate_optional_max_bytes(
            format!("commands[{index}].description").as_str(),
            command.description.as_deref(),
            4096,
            "description",
            issues,
        );
        validate_optional_max_bytes(
            format!("commands[{index}].argument_hint").as_str(),
            command.argument_hint.as_deref(),
            1024,
            "argument hint",
            issues,
        );
        if let Some(target_agent) = command.target_agent.as_deref() {
            if target_agent != SystemAgentKey::LocalAgentExecutionAgent.as_str() {
                issue(
                    issues,
                    format!("commands[{index}].target_agent").as_str(),
                    "target agent must be the execution system agent",
                );
            }
        }
        validate_allowed_tools(
            format!("commands[{index}].allowed_tools").as_str(),
            &command.allowed_tools,
            issues,
        );
    }
}

fn validate_agents(
    manifest: &PluginManifest,
    component_keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, agent) in manifest.agents.iter().enumerate() {
        validate_component_key(
            format!("agents[{index}].component_key"),
            agent.component_key.as_str(),
            component_keys,
            issues,
        );
        validate_path(format!("agents[{index}].source"), &agent.source, issues);
        validate_optional_max_bytes(
            format!("agents[{index}].description").as_str(),
            agent.description.as_deref(),
            4096,
            "description",
            issues,
        );
        if agent.base_agent != SystemAgentKey::LocalAgentExecutionAgent.as_str() {
            issue(
                issues,
                format!("agents[{index}].base_agent").as_str(),
                "base agent must be the execution system agent",
            );
        }
        validate_allowed_tools(
            format!("agents[{index}].allowed_tools").as_str(),
            &agent.allowed_tools,
            issues,
        );
        if !(1..=PLUGIN_AGENT_MAX_ITERATIONS).contains(&agent.max_iterations) {
            issue(
                issues,
                format!("agents[{index}].max_iterations").as_str(),
                format!("max iterations must be between 1 and {PLUGIN_AGENT_MAX_ITERATIONS}"),
            );
        }
    }
}

fn validate_hooks(
    manifest: &PluginManifest,
    component_keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, hook) in manifest.hooks.iter().enumerate() {
        validate_component_key(
            format!("hooks[{index}].component_key"),
            hook.component_key.as_str(),
            component_keys,
            issues,
        );
        validate_path(format!("hooks[{index}].source"), &hook.source, issues);
    }
}

fn validate_ui(
    manifest: &PluginManifest,
    component_keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, ui) in manifest.ui.iter().enumerate() {
        validate_component_key(
            format!("ui[{index}].component_key"),
            ui.component_key.as_str(),
            component_keys,
            issues,
        );
        validate_path(format!("ui[{index}].source"), &ui.source, issues);
        validate_ui_contribution(index, ui, issues);
    }
}

fn validate_runtime_context(
    manifest: &PluginManifest,
    component_keys: &HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let Some(context) = manifest.runtime_context.as_ref() else {
        return;
    };
    validate_runtime_context_components(context, component_keys, issues);

    let allowed_fields = [
        PLUGIN_RUNTIME_CONTEXT_FIELD_PROJECT_ID,
        PLUGIN_RUNTIME_CONTEXT_FIELD_WORKSPACE_ID,
        PLUGIN_RUNTIME_CONTEXT_FIELD_WORKSPACE_ROOT,
    ];
    if context.required.len() + context.optional.len() > PLUGIN_RUNTIME_CONTEXT_MAX_FIELDS {
        issue(
            issues,
            "runtimeContext",
            format!("at most {PLUGIN_RUNTIME_CONTEXT_MAX_FIELDS} context fields are allowed"),
        );
    }
    let mut seen_fields = HashSet::new();
    for (field_name, fields) in [
        ("runtimeContext.required", context.required.as_slice()),
        ("runtimeContext.optional", context.optional.as_slice()),
    ] {
        for (index, field) in fields.iter().enumerate() {
            if !allowed_fields.contains(&field.as_str()) {
                issue(
                    issues,
                    format!("{field_name}[{index}]").as_str(),
                    "unsupported runtime context field",
                );
            } else if !seen_fields.insert(field.as_str()) {
                issue(
                    issues,
                    format!("{field_name}[{index}]").as_str(),
                    "context field is duplicated or appears in both required and optional",
                );
            }
        }
    }

    if context.scope == PluginRuntimeContextScope::Device
        && seen_fields.iter().any(|field| {
            field.starts_with("workspace.") || *field == PLUGIN_RUNTIME_CONTEXT_FIELD_PROJECT_ID
        })
    {
        issue(
            issues,
            "runtimeContext.scope",
            "device scope cannot request project or workspace fields",
        );
    }
    if context.scope == PluginRuntimeContextScope::Workspace
        && seen_fields.contains(PLUGIN_RUNTIME_CONTEXT_FIELD_PROJECT_ID)
    {
        issue(
            issues,
            "runtimeContext.scope",
            "workspace scope cannot request project.id",
        );
    }
    if context.storage_isolation == PluginRuntimeContextStorageIsolation::Project
        && context.scope != PluginRuntimeContextScope::Project
    {
        issue(
            issues,
            "runtimeContext.storageIsolation",
            "project storage isolation requires project scope",
        );
    }
    if context.storage_isolation == PluginRuntimeContextStorageIsolation::Workspace
        && context.scope == PluginRuntimeContextScope::Device
    {
        issue(
            issues,
            "runtimeContext.storageIsolation",
            "workspace storage isolation requires workspace or project scope",
        );
    }
}

fn validate_runtime_context_components(
    context: &PluginRuntimeContext,
    component_keys: &HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if context.components.is_empty() {
        issue(
            issues,
            "runtimeContext.components",
            "at least one component is required",
        );
    }
    if context.components.len() > PLUGIN_RUNTIME_CONTEXT_MAX_COMPONENTS {
        issue(
            issues,
            "runtimeContext.components",
            format!("at most {PLUGIN_RUNTIME_CONTEXT_MAX_COMPONENTS} components are allowed"),
        );
    }
    let mut seen_components = HashSet::new();
    for (index, component) in context.components.iter().enumerate() {
        if !component_keys.contains(component) {
            issue(
                issues,
                format!("runtimeContext.components[{index}]").as_str(),
                "component is not declared by this plugin",
            );
        } else if !seen_components.insert(component.as_str()) {
            issue(
                issues,
                format!("runtimeContext.components[{index}]").as_str(),
                "duplicate component",
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

fn validate_optional_max_bytes(
    field: &str,
    value: Option<&str>,
    max_bytes: usize,
    label: &str,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if value.is_some_and(|value| value.len() > max_bytes) {
        issue(issues, field, format!("{label} exceeds {max_bytes} bytes"));
    }
}

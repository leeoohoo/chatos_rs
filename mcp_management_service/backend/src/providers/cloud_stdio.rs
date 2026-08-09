// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path};
use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, PluginMcpCloudRuntimeBundle, PluginMcpServer, ResolvedMcp,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::{CloudStdioProviderBinding, PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

mod manager_client;
mod prepare;
mod runtime_calls;
mod validation;
use validation::*;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SCOPE: &str = "sandbox.service";
const MAX_COMMAND_BYTES: usize = 256;
const MAX_TOOL_POLICY_ITEMS: usize = 512;
const MAX_TOOL_NAME_BYTES: usize = 256;

#[derive(Clone)]
pub(super) struct CloudStdioProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    response_limit_bytes: usize,
}

#[derive(Debug, Serialize)]
struct CloudStdioCallRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    invocation_id: Option<&'a str>,
    command: &'a str,
    args: &'a [String],
    env: &'a BTreeMap<String, String>,
    cwd: Option<&'a str>,
    plugin_artifact: Option<&'a PluginMcpCloudRuntimeBundle>,
    plugin_workspace_write: bool,
    method: &'a str,
    params: Value,
    expires_at_unix: i64,
    timeout_ms: u64,
}

#[derive(Debug, Serialize)]
struct CloudStdioCloseRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
}

#[derive(Debug, Serialize)]
struct CloudStdioCancelRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
    invocation_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct CloudStdioCallResponse {
    result: Value,
}

#[derive(Debug, Deserialize)]
struct CloudStdioCancelResponse {
    status: String,
}

impl CloudStdioProvider {
    pub(super) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Sandbox Manager cloud stdio base URL is invalid: {error}"))?;
        if !cfg!(test) && parsed.scheme() != "https" {
            return Err("Sandbox Manager cloud stdio base URL must use https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        self.internal_secret.is_some()
            && route.provider_kind == McpProviderKind::CloudStdio
            && route
                .provider_ref
                .as_deref()
                .is_some_and(|provider_ref| provider_ref.starts_with("sandbox:"))
    }

    pub(super) fn prepare_plugin_binding(
        &self,
        immutable: &PluginMcpRuntimeBinding,
        route: &ResolvedMcpRoute,
        resolved_environment: &BTreeMap<String, String>,
        runtime_bundle: &PluginMcpCloudRuntimeBundle,
    ) -> Result<CloudStdioProviderBinding, String> {
        if route.provider_kind != McpProviderKind::PluginCloud
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.resource_id != immutable.resource_id
            || route.allow_writes != immutable.allow_writes
        {
            return Err("Plugin stdio route does not match its immutable binding".to_string());
        }
        let PluginMcpServer::Stdio {
            command,
            args,
            env,
            cwd,
            ..
        } = runtime_bundle.effective_runtime()
        else {
            return Err("Plugin MCP runtime is not stdio".to_string());
        };
        if !immutable
            .permission_snapshot
            .iter()
            .any(|permission| permission == "process.spawn")
        {
            return Err(
                "Plugin stdio MCP requires process.spawn in its immutable permission snapshot"
                    .to_string(),
            );
        }
        let configured_names = env.keys().collect::<std::collections::BTreeSet<_>>();
        let resolved_names = resolved_environment
            .keys()
            .collect::<std::collections::BTreeSet<_>>();
        if configured_names != resolved_names {
            return Err(
                "Plugin stdio resolved environment does not match the immutable templates"
                    .to_string(),
            );
        }
        if !env.is_empty()
            && !immutable.permission_snapshot.iter().any(|permission| {
                permission == "credential.use" || permission.starts_with("credential.use:")
            })
        {
            return Err(
                "Plugin stdio credentials require credential.use in the immutable permission snapshot"
                    .to_string(),
            );
        }
        if runtime_bundle.bundle_sha256 != immutable.component_content_sha256
            || runtime_bundle.plugin_id != immutable.plugin_id
            || runtime_bundle.release_id != immutable.release_id
            || runtime_bundle.component.component_key != immutable.component_key
            || runtime_bundle.runtime != immutable.runtime
        {
            return Err("Plugin artifact Bundle does not match the immutable binding".to_string());
        }
        let package_relative_command = command.contains('/');
        let (command, cwd, plugin_artifact) = if package_relative_command {
            validate_plugin_artifact_ref(runtime_bundle.artifact_ref.as_str())?;
            let command = normalize_plugin_relative_path(command)
                .map_err(|error| format!("Plugin package-relative command is invalid: {error}"))?;
            let cwd = cwd
                .as_ref()
                .map(|value| normalize_plugin_relative_path(value.path.as_str()))
                .transpose()
                .map_err(|error| format!("Plugin package-relative cwd is invalid: {error}"))?;
            (command, cwd, Some(runtime_bundle.clone()))
        } else {
            if cwd.is_some() {
                return Err(
                    "Plugin package-relative cwd requires a package-relative executable"
                        .to_string(),
                );
            }
            validate_command(command, args.as_slice())?;
            (command.trim().to_string(), None, None)
        };
        validate_arguments(args.as_slice())?;
        validate_environment(resolved_environment)?;
        let allowed_tool_names =
            configured_tool_names(immutable.tool_allowlist.as_slice(), "tool_allowlist")?;
        let blocked_tool_names =
            configured_tool_names(immutable.tool_blocklist.as_slice(), "tool_blocklist")?;
        if !route.allow_writes && allowed_tool_names.is_empty() {
            return Err("read-only Plugin stdio MCP requires tool_allowlist".to_string());
        }
        Ok(CloudStdioProviderBinding {
            provider_ref: immutable.provider_ref.clone(),
            command,
            args: args.clone(),
            env: resolved_environment.clone(),
            cwd,
            plugin_artifact,
            allow_writes: route.allow_writes,
            allowed_tool_names,
            blocked_tool_names,
        })
    }
}

#[cfg(test)]
mod tests;

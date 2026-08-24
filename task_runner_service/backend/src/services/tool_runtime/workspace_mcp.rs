// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    normalize_external_mcp_config_ids, TaskMcpConfig, TaskMcpResolutionResponse, TaskRecord,
};
use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};

use super::mcp_resolution::{
    resolve_task_mcp, selected_builtin_kinds_from_config,
    task_mcp_resolution_response as build_mcp_resolution_response,
};
use super::normalize_strings;
use super::normalized_optional;

#[path = "workspace_mcp/workspace_dirs.rs"]
mod workspace_dirs;

#[cfg(test)]
#[path = "workspace_mcp/tests.rs"]
mod tests;

#[cfg(test)]
use workspace_dirs::ensure_workspace_is_inside_base;
pub(super) use workspace_dirs::{
    default_user_workspace_dir, ensure_effective_task_workspace_dir,
    ensure_workspace_dir_available, resolve_workspace_dir_with_base,
};

pub(super) fn selected_builtin_kinds(mcp_config: &TaskMcpConfig) -> Vec<BuiltinMcpKind> {
    selected_builtin_kinds_from_config(mcp_config)
}

pub(super) fn runtime_selected_builtin_kinds(task: &TaskRecord) -> Vec<BuiltinMcpKind> {
    resolve_task_mcp(task, &[]).server_local_builtin_kinds
}

pub(super) fn task_mcp_resolution_response(task: &TaskRecord) -> TaskMcpResolutionResponse {
    // Task Runner reports capability selection only. MCP Management resolves the
    // authoritative provider route from the project execution context.
    build_mcp_resolution_response(task, &[])
}

pub(super) fn normalize_builtin_kind_names(values: Vec<String>) -> Vec<String> {
    let mut kinds = Vec::new();
    for value in values {
        let Some(kind) = builtin_kind_by_any(&value) else {
            continue;
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

pub(super) fn sanitize_task_mcp_config(mut config: TaskMcpConfig) -> TaskMcpConfig {
    config.init_mode = chatos_ai_runtime::TaskMcpInitMode::Full;
    config.builtin_prompt_locale = normalized_optional(Some(config.builtin_prompt_locale))
        .unwrap_or_else(|| chatos_mcp_runtime::BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
    config.enabled_builtin_kinds = normalize_builtin_kind_names(config.enabled_builtin_kinds);
    config
        .enabled_builtin_kinds
        .retain(|kind| kind != BuiltinMcpKind::RemoteConnectionController.kind_name());
    config.workspace_dir = normalized_optional(config.workspace_dir);
    config.execution_service_id = normalized_optional(config.execution_service_id);
    config.external_mcp_config_ids =
        normalize_external_mcp_config_ids(config.external_mcp_config_ids);
    config.selected_skill_ids = normalize_strings(config.selected_skill_ids);
    config.skill_policy_revision = normalized_optional(config.skill_policy_revision);
    // Runtime provider endpoints are never persisted in a task. All actual MCP
    // endpoints are materialized by MCP Management for a bound runtime session.
    config.ephemeral_http_servers.clear();
    config
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};
use chatos_mcp_service::{
    split_builtin_kind_header, BuiltinHostBackend, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};

use crate::models::{
    TaskMcpConfig, TaskMcpHostedBuiltinRoute, TaskMcpRequiredBuiltinCapability,
    TaskMcpResolutionResponse, TaskRecord, TaskScheduleMode, TASK_PROFILE_CHATOS_PLAN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(super) enum AgentMcpCaller {
    ChatosAsyncPlanner,
    ProjectManagementAgent,
    LocalConnectorClientAgent,
    TaskRunnerRunPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(super) enum McpCapabilityRequirementSource {
    CallerContract(AgentMcpCaller),
    TaskProfileChatosPlan,
    RuntimeInternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct McpCapabilityRequirement {
    pub kind: BuiltinMcpKind,
    pub source: McpCapabilityRequirementSource,
}

impl McpCapabilityRequirement {
    pub fn new(kind: BuiltinMcpKind, source: McpCapabilityRequirementSource) -> Self {
        Self { kind, source }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RequiredBuiltinCapability {
    pub kind: BuiltinMcpKind,
    pub source: McpCapabilityRequirementSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HostedBuiltinRoute {
    pub host: BuiltinHostBackend,
    pub builtin_kinds: Vec<BuiltinMcpKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskMcpResolution {
    pub requested_builtin_kinds: Vec<BuiltinMcpKind>,
    pub required_builtin_kinds: Vec<RequiredBuiltinCapability>,
    pub hosted_builtin_routes: Vec<HostedBuiltinRoute>,
    pub server_local_builtin_kinds: Vec<BuiltinMcpKind>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TaskMcpResolutionInput<'a> {
    pub mcp_config: &'a TaskMcpConfig,
    pub task_profile: &'a str,
    pub schedule_mode: TaskScheduleMode,
    pub source_session_id: Option<&'a str>,
    pub source_user_message_id: Option<&'a str>,
    pub active_host_backends: &'a [BuiltinHostBackend],
    pub caller_requirements: &'a [McpCapabilityRequirement],
}

pub(super) fn selected_builtin_kinds_from_config(
    mcp_config: &TaskMcpConfig,
) -> Vec<BuiltinMcpKind> {
    let mut kinds = mcp_config
        .enabled_builtin_kinds
        .iter()
        .filter_map(|value| builtin_kind_by_any(value))
        .collect::<Vec<_>>();
    kinds.extend(hosted_builtin_kinds_from_config(mcp_config));
    complete_builtin_kind_dependencies(kinds)
}

fn hosted_builtin_kinds_from_config(mcp_config: &TaskMcpConfig) -> Vec<BuiltinMcpKind> {
    let mut out = Vec::new();
    for server in &mcp_config.ephemeral_http_servers {
        push_hosted_builtin_kinds_from_header(
            &mut out,
            server
                .headers
                .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER),
            BuiltinHostBackend::LocalConnector,
        );
        push_hosted_builtin_kinds_from_header(
            &mut out,
            server
                .headers
                .get(HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER),
            BuiltinHostBackend::HarnessCode,
        );
    }
    out
}

fn push_hosted_builtin_kinds_from_header(
    out: &mut Vec<BuiltinMcpKind>,
    raw: Option<&String>,
    host: BuiltinHostBackend,
) {
    let Some(raw) = raw else {
        return;
    };
    for value in split_builtin_kind_header(raw) {
        let Some(kind) = builtin_kind_by_any(value) else {
            continue;
        };
        if host.replaces_builtin_kind_name(kind.kind_name()) && !out.contains(&kind) {
            out.push(kind);
        }
    }
}

pub(super) fn resolve_task_mcp(
    task: &TaskRecord,
    active_host_backends: &[BuiltinHostBackend],
) -> TaskMcpResolution {
    let caller_requirements =
        caller_builtin_capability_requirements(AgentMcpCaller::TaskRunnerRunPhase);
    resolve_task_mcp_with_requirements(task, active_host_backends, caller_requirements.as_slice())
}

pub(super) fn resolve_task_mcp_authoritative(
    task: &TaskRecord,
    active_host_backends: &[BuiltinHostBackend],
) -> TaskMcpResolution {
    let effective_kinds = selected_builtin_kinds_from_config(&task.mcp_config);
    let hosted_builtin_routes = hosted_builtin_routes(&effective_kinds, active_host_backends);
    let server_local_builtin_kinds =
        server_local_builtin_kinds(effective_kinds.clone(), active_host_backends);
    TaskMcpResolution {
        requested_builtin_kinds: effective_kinds,
        required_builtin_kinds: Vec::new(),
        hosted_builtin_routes,
        server_local_builtin_kinds,
    }
}

pub(super) fn resolve_task_mcp_with_requirements(
    task: &TaskRecord,
    active_host_backends: &[BuiltinHostBackend],
    caller_requirements: &[McpCapabilityRequirement],
) -> TaskMcpResolution {
    resolve_mcp_config(TaskMcpResolutionInput {
        mcp_config: &task.mcp_config,
        task_profile: task.task_profile.as_str(),
        schedule_mode: task.schedule.mode,
        source_session_id: task.source_session_id.as_deref(),
        source_user_message_id: task.source_user_message_id.as_deref(),
        active_host_backends,
        caller_requirements,
    })
}

pub(super) fn resolve_mcp_config(input: TaskMcpResolutionInput<'_>) -> TaskMcpResolution {
    let mut requested_builtin_kinds = selected_builtin_kinds_from_config(input.mcp_config);
    requested_builtin_kinds.retain(|kind| {
        !matches!(
            kind,
            BuiltinMcpKind::ProjectManagement
                | BuiltinMcpKind::TaskManager
                | BuiltinMcpKind::AskUser
        )
    });
    let required_builtin_kinds = required_builtin_capabilities(input);
    let required_kinds = required_builtin_kinds
        .iter()
        .map(|requirement| requirement.kind)
        .collect::<Vec<_>>();

    let mut effective_kinds = if is_chatos_plan_profile(input.task_profile) {
        required_kinds
    } else {
        requested_builtin_kinds
            .iter()
            .copied()
            .chain(required_kinds)
            .collect::<Vec<_>>()
    };
    effective_kinds = complete_builtin_kind_dependencies(effective_kinds);

    let hosted_builtin_routes = hosted_builtin_routes(&effective_kinds, input.active_host_backends);
    let server_local_builtin_kinds =
        server_local_builtin_kinds(effective_kinds, input.active_host_backends);

    TaskMcpResolution {
        requested_builtin_kinds,
        required_builtin_kinds,
        hosted_builtin_routes,
        server_local_builtin_kinds,
    }
}

pub(super) fn hosted_builtin_kinds_for(
    resolution: &TaskMcpResolution,
    host: BuiltinHostBackend,
) -> Vec<BuiltinMcpKind> {
    resolution
        .hosted_builtin_routes
        .iter()
        .find(|route| route.host == host)
        .map(|route| route.builtin_kinds.clone())
        .unwrap_or_default()
}

pub(super) fn task_mcp_resolution_response(
    task: &TaskRecord,
    active_host_backends: &[BuiltinHostBackend],
) -> TaskMcpResolutionResponse {
    let resolution = resolve_task_mcp(task, active_host_backends);
    TaskMcpResolutionResponse {
        requested_builtin_kinds: kind_names(resolution.requested_builtin_kinds),
        required_builtin_kinds: resolution
            .required_builtin_kinds
            .into_iter()
            .map(|required| TaskMcpRequiredBuiltinCapability {
                kind: required.kind.kind_name().to_string(),
                source: requirement_source_key(required.source).to_string(),
            })
            .collect(),
        hosted_builtin_routes: resolution
            .hosted_builtin_routes
            .into_iter()
            .map(|route| TaskMcpHostedBuiltinRoute {
                host: host_key(route.host).to_string(),
                server_name: host_server_name(route.host).to_string(),
                public_server_names: public_server_names(route.builtin_kinds.as_slice()),
                builtin_kinds: kind_names(route.builtin_kinds),
            })
            .collect(),
        server_local_builtin_kinds: kind_names(resolution.server_local_builtin_kinds),
        external_mcp_config_ids: task.mcp_config.external_mcp_config_ids.clone(),
    }
}

fn kind_names(kinds: Vec<BuiltinMcpKind>) -> Vec<String> {
    kinds
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

fn requirement_source_key(source: McpCapabilityRequirementSource) -> &'static str {
    match source {
        McpCapabilityRequirementSource::CallerContract(AgentMcpCaller::ChatosAsyncPlanner) => {
            "chatos_async_planner"
        }
        McpCapabilityRequirementSource::CallerContract(AgentMcpCaller::ProjectManagementAgent) => {
            "project_management_agent"
        }
        McpCapabilityRequirementSource::CallerContract(
            AgentMcpCaller::LocalConnectorClientAgent,
        ) => "local_connector_client_agent",
        McpCapabilityRequirementSource::CallerContract(AgentMcpCaller::TaskRunnerRunPhase) => {
            "task_runner_run_phase"
        }
        McpCapabilityRequirementSource::TaskProfileChatosPlan => "task_profile_chatos_plan",
        McpCapabilityRequirementSource::RuntimeInternal => "runtime_internal",
    }
}

fn host_key(host: BuiltinHostBackend) -> &'static str {
    match host {
        BuiltinHostBackend::LocalConnector => "local_connector",
        BuiltinHostBackend::HarnessCode => "harness_code",
    }
}

fn host_server_name(host: BuiltinHostBackend) -> &'static str {
    host_key(host)
}

fn public_server_names(kinds: &[BuiltinMcpKind]) -> Vec<String> {
    let mut out = Vec::new();
    for kind in kinds {
        let server_name = kind.server_name().to_string();
        if !out.contains(&server_name) {
            out.push(server_name);
        }
    }
    out
}

fn required_builtin_capabilities(
    input: TaskMcpResolutionInput<'_>,
) -> Vec<RequiredBuiltinCapability> {
    let mut requirements = Vec::new();
    if is_chatos_plan_profile(input.task_profile) {
        requirements.extend(chatos_plan_profile_requirements());
    }
    if is_chatos_async_context(input) {
        requirements.extend(chatos_async_planner_requirements());
    }
    requirements.extend(input.caller_requirements.iter().copied());
    complete_required_dependencies(requirements)
}

pub(super) fn caller_builtin_capability_requirements(
    caller: AgentMcpCaller,
) -> Vec<McpCapabilityRequirement> {
    use AgentMcpCaller::*;
    use BuiltinMcpKind::*;

    let kinds: &[BuiltinMcpKind] = match caller {
        ChatosAsyncPlanner | TaskRunnerRunPhase => &[TaskManager, AskUser],
        ProjectManagementAgent => &[ProjectManagement],
        LocalConnectorClientAgent => &[],
    };
    kinds
        .iter()
        .copied()
        .map(|kind| {
            McpCapabilityRequirement::new(
                kind,
                McpCapabilityRequirementSource::CallerContract(caller),
            )
        })
        .collect()
}

fn chatos_plan_profile_requirements() -> Vec<McpCapabilityRequirement> {
    use BuiltinMcpKind::*;
    [
        CodeMaintainerRead,
        TerminalController,
        TaskManager,
        ProjectManagement,
        Notepad,
        AskUser,
        RemoteConnectionController,
        WebTools,
        BrowserTools,
        MemorySkillReader,
        MemoryCommandReader,
        MemoryPluginReader,
    ]
    .into_iter()
    .map(|kind| {
        McpCapabilityRequirement::new(kind, McpCapabilityRequirementSource::TaskProfileChatosPlan)
    })
    .collect()
}

fn chatos_async_planner_requirements() -> Vec<McpCapabilityRequirement> {
    caller_builtin_capability_requirements(AgentMcpCaller::ChatosAsyncPlanner)
}

fn complete_required_dependencies(
    requirements: Vec<McpCapabilityRequirement>,
) -> Vec<RequiredBuiltinCapability> {
    let mut out = Vec::new();
    for requirement in requirements {
        push_required(&mut out, requirement.kind, requirement.source);
        if requirement.kind == BuiltinMcpKind::CodeMaintainerWrite {
            push_required(
                &mut out,
                BuiltinMcpKind::CodeMaintainerRead,
                requirement.source,
            );
        }
    }
    out
}

fn push_required(
    out: &mut Vec<RequiredBuiltinCapability>,
    kind: BuiltinMcpKind,
    source: McpCapabilityRequirementSource,
) {
    if !out.iter().any(|existing| existing.kind == kind) {
        out.push(RequiredBuiltinCapability { kind, source });
    }
}

fn hosted_builtin_routes(
    effective_kinds: &[BuiltinMcpKind],
    active_host_backends: &[BuiltinHostBackend],
) -> Vec<HostedBuiltinRoute> {
    active_host_backends
        .iter()
        .copied()
        .filter_map(|host| {
            let builtin_kinds = effective_kinds
                .iter()
                .copied()
                .filter(|kind| host.replaces_builtin_kind_name(kind.kind_name()))
                .collect::<Vec<_>>();
            if builtin_kinds.is_empty() {
                None
            } else {
                Some(HostedBuiltinRoute {
                    host,
                    builtin_kinds,
                })
            }
        })
        .collect()
}

fn server_local_builtin_kinds(
    effective_kinds: Vec<BuiltinMcpKind>,
    active_host_backends: &[BuiltinHostBackend],
) -> Vec<BuiltinMcpKind> {
    let mut kinds = effective_kinds;
    remove_hosted_builtin_kinds(&mut kinds, active_host_backends);
    kinds = complete_builtin_kind_dependencies(kinds);
    remove_hosted_builtin_kinds(&mut kinds, active_host_backends);
    kinds
}

fn remove_hosted_builtin_kinds(
    kinds: &mut Vec<BuiltinMcpKind>,
    active_host_backends: &[BuiltinHostBackend],
) {
    kinds.retain(|kind| {
        !active_host_backends
            .iter()
            .any(|host| host.replaces_builtin_kind_name(kind.kind_name()))
    });
}

fn is_chatos_plan_profile(task_profile: &str) -> bool {
    task_profile
        .trim()
        .eq_ignore_ascii_case(TASK_PROFILE_CHATOS_PLAN)
}

fn is_chatos_async_context(input: TaskMcpResolutionInput<'_>) -> bool {
    input.schedule_mode == TaskScheduleMode::ContactAsync
        || (has_non_empty_text(input.source_session_id)
            && has_non_empty_text(input.source_user_message_id))
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;

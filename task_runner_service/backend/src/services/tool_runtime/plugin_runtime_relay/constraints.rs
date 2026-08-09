// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use chatos_plugin_management_sdk::{
    PluginComponentKind, SystemAgentKey, PLUGIN_AGENT_MAX_ITERATIONS,
    PLUGIN_COMMAND_MAX_ALLOWED_TOOLS, PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES,
};
use serde_json::Value;

use crate::models::TaskRunRecord;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::services) struct PluginCommandExecutionConstraints {
    pub target_agent: Option<String>,
    pub tool_allowlists: Vec<Vec<String>>,
    pub agent_identity: Option<String>,
    pub max_iterations: Option<usize>,
}

pub(in crate::services) fn plugin_command_execution_constraints(
    run: &TaskRunRecord,
) -> Result<PluginCommandExecutionConstraints, String> {
    let mut constraints = PluginCommandExecutionConstraints::default();
    for plugin in &run.plugin_snapshots {
        for component in &plugin.component_snapshots {
            let identity = format!("{}:{}", plugin.plugin_id, component.component_key);
            match component.kind {
                PluginComponentKind::Command => {
                    let metadata = match component.runtime.get("metadata") {
                        Some(value) => Some(value.as_object().ok_or_else(|| {
                            format!("Plugin Command metadata must be an object: {identity}")
                        })?),
                        None => None,
                    };
                    let target_agent = command_target_agent(
                        metadata.and_then(|value| value.get("target_agent")),
                        identity.as_str(),
                    )?;
                    merge_target_agent(&mut constraints, target_agent, "Plugin Command")?;
                    let allowed_tools = component_allowed_tools(
                        metadata.and_then(|value| value.get("allowed_tools")),
                        identity.as_str(),
                        "Plugin Command",
                    )?;
                    if !allowed_tools.is_empty() {
                        constraints
                            .tool_allowlists
                            .push(allowed_tools.into_iter().collect());
                    }
                }
                PluginComponentKind::Agent => {
                    if let Some(existing) = constraints.agent_identity.as_deref() {
                        return Err(format!(
                            "a Run may select at most one Plugin Agent: {existing} and {identity}"
                        ));
                    }
                    let metadata = component
                        .runtime
                        .get("metadata")
                        .and_then(Value::as_object)
                        .ok_or_else(|| {
                            format!("Plugin Agent metadata must be an object: {identity}")
                        })?;
                    let base_agent =
                        agent_base_agent(metadata.get("base_agent"), identity.as_str())?;
                    merge_target_agent(&mut constraints, Some(base_agent), "Plugin Agent")?;
                    let allowed_tools = component_allowed_tools(
                        metadata.get("allowed_tools"),
                        identity.as_str(),
                        "Plugin Agent",
                    )?;
                    if !allowed_tools.is_empty() {
                        constraints
                            .tool_allowlists
                            .push(allowed_tools.into_iter().collect());
                    }
                    constraints.max_iterations = Some(agent_max_iterations(
                        metadata.get("max_iterations"),
                        identity.as_str(),
                    )?);
                    constraints.agent_identity = Some(identity);
                }
                _ => {}
            }
        }
    }
    Ok(constraints)
}

pub(super) fn merge_target_agent(
    constraints: &mut PluginCommandExecutionConstraints,
    target_agent: Option<&str>,
    component_label: &str,
) -> Result<(), String> {
    let Some(target_agent) = target_agent else {
        return Ok(());
    };
    if let Some(existing) = constraints.target_agent.as_deref() {
        if existing != target_agent {
            return Err(format!(
                "selected Plugin components require different target Agents: {existing} and {target_agent} ({component_label})"
            ));
        }
    } else {
        constraints.target_agent = Some(target_agent.to_string());
    }
    Ok(())
}

pub(super) fn agent_base_agent<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<&'a str, String> {
    let raw_base_agent = value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Plugin Agent base Agent is missing or invalid in {context}"))?;
    let base_agent = raw_base_agent.trim();
    if base_agent.is_empty() || base_agent != raw_base_agent {
        return Err(format!(
            "Plugin Agent base Agent is not normalized in {context}"
        ));
    }
    if ![
        SystemAgentKey::TaskRunnerPlanPhase.as_str(),
        SystemAgentKey::TaskRunnerRunPhase.as_str(),
    ]
    .contains(&base_agent)
    {
        return Err(format!(
            "Plugin Agent base Agent is unsupported in {context}: {base_agent}"
        ));
    }
    Ok(base_agent)
}

pub(super) fn agent_max_iterations(value: Option<&Value>, context: &str) -> Result<usize, String> {
    let value = value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("Plugin Agent max iterations is missing or invalid in {context}"))?;
    if !(1..=PLUGIN_AGENT_MAX_ITERATIONS).contains(&value) {
        return Err(format!(
            "Plugin Agent max iterations must be between 1 and {PLUGIN_AGENT_MAX_ITERATIONS} in {context}"
        ));
    }
    Ok(value)
}

pub(super) fn command_target_agent<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<Option<&'a str>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let raw_target_agent = value
        .as_str()
        .ok_or_else(|| format!("Plugin Command target Agent is invalid in {context}"))?;
    let target_agent = raw_target_agent.trim();
    if target_agent.is_empty() || target_agent != raw_target_agent {
        return Err(format!(
            "Plugin Command target Agent is not normalized in {context}"
        ));
    }
    if ![
        SystemAgentKey::TaskRunnerPlanPhase.as_str(),
        SystemAgentKey::TaskRunnerRunPhase.as_str(),
    ]
    .contains(&target_agent)
    {
        return Err(format!(
            "Plugin Command target Agent is unsupported in {context}: {target_agent}"
        ));
    }
    Ok(Some(target_agent))
}

pub(super) fn component_allowed_tools(
    value: Option<&Value>,
    context: &str,
    component_label: &str,
) -> Result<BTreeSet<String>, String> {
    let Some(value) = value else {
        return Ok(BTreeSet::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("{component_label} allowed tools must be an array in {context}"))?;
    if values.len() > PLUGIN_COMMAND_MAX_ALLOWED_TOOLS {
        return Err(format!(
            "{component_label} allowed tools exceed {PLUGIN_COMMAND_MAX_ALLOWED_TOOLS} items in {context}"
        ));
    }
    let mut allowed_tools = BTreeSet::new();
    for value in values {
        let raw_tool_name = value
            .as_str()
            .ok_or_else(|| format!("{component_label} allowed tool is invalid in {context}"))?;
        let tool_name = raw_tool_name.trim();
        if tool_name.is_empty() || tool_name != raw_tool_name {
            return Err(format!(
                "{component_label} allowed tool is not normalized in {context}"
            ));
        }
        if tool_name.len() > PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES
            || !tool_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(format!(
                "{component_label} allowed tool is not a canonical public tool name in {context}: {tool_name}"
            ));
        }
        if !allowed_tools.insert(tool_name.to_string()) {
            return Err(format!(
                "{component_label} allowed tool is duplicated in {context}: {tool_name}"
            ));
        }
    }
    Ok(allowed_tools)
}

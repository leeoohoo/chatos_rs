// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Declarative system-tool catalog owned by Plugin Management.
//!
//! This module intentionally contains no executor, process transport, remote
//! relay, or tool implementation. Local clients discover executable tools from
//! their installed capability registry; the service stores only stable catalog
//! identity, policy metadata, and the two run-scoped schemas it owns.

use chatos_plugin_management_sdk::{
    McpProviderSkill, McpRecord, SystemMcpKey, LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
    SYSTEM_MCP_RUNTIME_KIND, TASK_PROCESS_LOG_MCP_RESOURCE_ID,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SystemMcpDescriptor {
    pub key: SystemMcpKey,
    pub resource_id: &'static str,
    pub server_name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub allow_writes: bool,
    pub tags: &'static [&'static str],
    pub category: Option<&'static str>,
    pub owner_service: &'static str,
    pub runtime_command: Option<&'static str>,
}

macro_rules! descriptor {
    ($key:ident, $resource_id:expr, $server_name:expr, $display_name:expr, $description:expr, $allow_writes:expr, $tags:expr, $category:expr, $owner:expr, $command:expr) => {
        SystemMcpDescriptor {
            key: SystemMcpKey::$key,
            resource_id: $resource_id,
            server_name: $server_name,
            display_name: $display_name,
            description: $description,
            allow_writes: $allow_writes,
            tags: $tags,
            category: $category,
            owner_service: $owner,
            runtime_command: $command,
        }
    };
}

static SYSTEM_MCP_CATALOG: [SystemMcpDescriptor; 12] = [
    descriptor!(
        CodeMaintainerRead,
        "builtin_code_maintainer_read",
        "code_maintainer_read",
        "Code Maintainer Read",
        "Read-only project inspection tools provided by the native client.",
        false,
        &["system", "local"],
        Some("developer"),
        "native_client",
        Some("builtin:code_maintainer_read")
    ),
    descriptor!(
        CodeMaintainerWrite,
        "builtin_code_maintainer_write",
        "code_maintainer_write",
        "Code Maintainer Write",
        "Project editing tools provided by the native client.",
        true,
        &["system", "local"],
        Some("developer"),
        "native_client",
        Some("builtin:code_maintainer_write")
    ),
    descriptor!(
        TerminalController,
        "builtin_terminal_controller",
        "terminal_controller",
        "Terminal Controller",
        "Managed terminal tools provided by the native client.",
        true,
        &["system", "local"],
        Some("developer"),
        "native_client",
        Some("builtin:terminal_controller")
    ),
    descriptor!(
        Notepad,
        "builtin_notepad",
        "notepad",
        "Notepad",
        "Persistent notepad tools.",
        true,
        &["system"],
        Some("productivity"),
        "chatos",
        Some("builtin:notepad")
    ),
    descriptor!(
        AgentBuilder,
        "builtin_agent_builder",
        "agent_builder",
        "Agent Builder",
        "Agent configuration and skill composition tools.",
        true,
        &["system"],
        Some("agent"),
        "chatos",
        Some("builtin:agent_builder")
    ),
    descriptor!(
        AskUser,
        "builtin_ask_user",
        "ask_user",
        "Ask User",
        "Structured user clarification tools provided by the native client.",
        true,
        &["system", "local"],
        Some("collaboration"),
        "native_client",
        Some("builtin:ask_user")
    ),
    descriptor!(
        RemoteConnectionController,
        "builtin_remote_connection_controller",
        "remote_connection_controller",
        "Remote Connection Controller",
        "Remote connection tools provided by an installed local capability.",
        true,
        &["system", "local", "remote_connection"],
        Some("developer"),
        "native_client",
        Some("builtin:remote_connection_controller")
    ),
    descriptor!(
        MemorySkillReader,
        "system_builtin_memory_skill_reader",
        "memory_skill_reader",
        "Memory Skill Reader",
        "Read agent skills from assembled memory context.",
        false,
        &["system", "memory"],
        Some("memory"),
        "native_client",
        None
    ),
    descriptor!(
        MemoryCommandReader,
        "system_builtin_memory_command_reader",
        "memory_command_reader",
        "Memory Command Reader",
        "Read agent commands from assembled memory context.",
        false,
        &["system", "memory"],
        Some("memory"),
        "native_client",
        None
    ),
    descriptor!(
        MemoryPluginReader,
        "system_builtin_memory_plugin_reader",
        "memory_plugin_reader",
        "Memory Plugin Reader",
        "Read agent plugin memory from assembled context.",
        false,
        &["system", "memory"],
        Some("memory"),
        "native_client",
        None
    ),
    descriptor!(
        LocalCommandApproval,
        LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
        "local_command_approval",
        "Local Command Approval",
        "Final decision tool for project command approval.",
        true,
        &["system", "local", "approval"],
        Some("security"),
        "native_client",
        None
    ),
    descriptor!(
        TaskProcessLog,
        TASK_PROCESS_LOG_MCP_RESOURCE_ID,
        "task_run_process",
        "Task Process Log",
        "Run-scoped tools for visible task progress and final outcome.",
        true,
        &["system", "local", "process_log", "run_scoped"],
        Some("task"),
        "native_client",
        None
    ),
];

pub(crate) fn system_mcp_catalog() -> &'static [SystemMcpDescriptor] {
    &SYSTEM_MCP_CATALOG
}

pub(crate) fn system_mcp_descriptor(key: SystemMcpKey) -> &'static SystemMcpDescriptor {
    SYSTEM_MCP_CATALOG
        .iter()
        .find(|descriptor| descriptor.key == key)
        .expect("every active SystemMcpKey must have a descriptor")
}

#[cfg(test)]
pub(crate) fn system_mcp_descriptor_by_resource_id(
    value: &str,
) -> Option<&'static SystemMcpDescriptor> {
    let value = value.trim();
    SYSTEM_MCP_CATALOG
        .iter()
        .find(|descriptor| descriptor.resource_id == value)
}

pub(crate) fn system_mcp_descriptor_by_any(value: &str) -> Option<&'static SystemMcpDescriptor> {
    let value = value.trim();
    SYSTEM_MCP_CATALOG.iter().find(|descriptor| {
        descriptor.resource_id == value
            || descriptor.server_name == value
            || descriptor.key.as_str() == value
            || descriptor.runtime_command == Some(value)
    })
}

pub(crate) fn system_mcp_descriptor_for_record(
    record: &McpRecord,
) -> Option<&'static SystemMcpDescriptor> {
    if record.runtime.kind != SYSTEM_MCP_RUNTIME_KIND {
        return None;
    }
    record
        .runtime
        .system_key
        .as_deref()
        .or(record.runtime.server_name.as_deref())
        .and_then(system_mcp_descriptor_by_any)
        .or_else(|| system_mcp_descriptor_by_any(record.id.as_str()))
        .or_else(|| system_mcp_descriptor_by_any(record.name.as_str()))
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SystemMcpToolCatalog {
    Static(Vec<Value>),
    Dynamic,
}

pub(crate) fn system_mcp_tool_catalog(key: SystemMcpKey) -> Result<SystemMcpToolCatalog, String> {
    Ok(match key {
        SystemMcpKey::LocalCommandApproval => {
            SystemMcpToolCatalog::Static(vec![approval_decision_tool()])
        }
        SystemMcpKey::TaskProcessLog => SystemMcpToolCatalog::Static(task_process_tools()),
        _ => SystemMcpToolCatalog::Dynamic,
    })
}

pub(crate) fn system_mcp_provider_skills(key: SystemMcpKey) -> Vec<McpProviderSkill> {
    let descriptor = system_mcp_descriptor(key);
    [
        (
            "zh-CN",
            "使用指南",
            format!(
                "只使用当前运行实际暴露的 {} 工具。调用前确认参数、项目范围和写入影响；工具不可用时直接说明，不得臆造执行结果。",
                descriptor.display_name
            ),
        ),
        (
            "en-US",
            "Usage Guide",
            format!(
                "Use only the {} tools actually exposed in the current run. Validate arguments, project scope, and write effects before calling; never invent execution results when a tool is unavailable.",
                descriptor.display_name
            ),
        ),
    ]
    .into_iter()
    .map(|(locale, suffix, instructions)| McpProviderSkill {
        id: format!("{}_usage_{}", descriptor.server_name, locale.to_ascii_lowercase()),
        name: format!("{} {suffix}", descriptor.display_name),
        description: descriptor.description.to_string(),
        instructions,
        locale: Some(locale.to_string()),
        task_profiles: Vec::new(),
    })
    .collect()
}

fn approval_decision_tool() -> Value {
    json!({
        "name": "approval_decision",
        "description": "Return the final command approval decision for this request.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "decision": { "type": "string", "enum": ["approve", "deny", "ask_user"] },
                "reason": { "type": "string" },
                "remember_allow": { "type": "boolean" }
            },
            "required": ["decision", "reason"],
            "additionalProperties": false
        }
    })
}

fn task_process_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "record_process",
            "description": "Record a short user-visible progress entry for the current task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operation": { "type": "string", "enum": ["append", "replace", "clear"] },
                    "heading": { "type": ["string", "null"] },
                    "content": { "type": ["string", "null"] }
                },
                "required": ["operation", "heading", "content"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "report_outcome",
            "description": "Report the final status for the current task after verification.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["succeeded", "failed", "blocked"] },
                    "reason": { "type": "string", "minLength": 1, "maxLength": 2000 }
                },
                "required": ["status", "reason"],
                "additionalProperties": false
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_has_one_descriptor_per_active_key() {
        let keys = system_mcp_catalog()
            .iter()
            .map(|descriptor| descriptor.key)
            .collect::<HashSet<_>>();
        assert_eq!(keys.len(), SystemMcpKey::ALL.len());
    }

    #[test]
    fn declarative_catalog_contains_no_executor_configuration() {
        for descriptor in system_mcp_catalog() {
            assert!(!descriptor.owner_service.trim().is_empty());
        }
    }
}

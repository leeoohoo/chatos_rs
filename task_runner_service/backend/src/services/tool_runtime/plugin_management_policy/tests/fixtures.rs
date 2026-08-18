// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolState,
};

use super::super::*;

#[path = "fixtures/base_plugin.rs"]
mod base_plugin;
#[path = "fixtures/component_plugins.rs"]
mod component_plugins;
#[path = "fixtures/core.rs"]
mod core;

pub(super) use base_plugin::resolved_plugin;
pub(super) use component_plugins::{resolved_agent_plugin, resolved_command_plugin};
pub(super) use core::{resolved_mcp, resolved_skill};

pub(super) fn policy() -> TaskRunnerCapabilityPolicy {
    TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![
            resolved_mcp(
                "ask-user",
                BUILTIN_RUNTIME_KIND,
                Some("AskUser"),
                true,
                true,
            ),
            resolved_mcp(
                chatos_plugin_management_sdk::TASK_PROCESS_LOG_MCP_RESOURCE_ID,
                chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND,
                Some("task_process_log"),
                true,
                true,
            ),
            resolved_mcp(
                "read",
                BUILTIN_RUNTIME_KIND,
                Some("CodeMaintainerRead"),
                false,
                true,
            ),
            resolved_mcp(
                "write",
                BUILTIN_RUNTIME_KIND,
                Some("CodeMaintainerWrite"),
                false,
                false,
            ),
            resolved_mcp("external-1", "http", None, false, true),
        ],
        skills: vec![resolved_skill("internal_skill_remotion", false, true)],
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    })
    .expect("policy")
}

pub(super) fn task() -> TaskRecord {
    let now = now_rfc3339();
    TaskRecord {
        id: "task-1".to_string(),
        title: "Task".to_string(),
        description: None,
        objective: "Objective".to_string(),
        input_payload: None,
        status: TaskStatus::Ready,
        priority: 0,
        tags: Vec::new(),
        default_model_config_id: None,
        memory_thread_id: "thread-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        subject_id: "owner-1".to_string(),
        project_id: "public".to_string(),
        task_profile: "default".to_string(),
        creator_user_id: Some("owner-1".to_string()),
        creator_username: None,
        creator_display_name: None,
        owner_user_id: Some("owner-1".to_string()),
        owner_username: None,
        owner_display_name: None,
        result_summary: None,
        process_log: None,
        last_run_id: None,
        schedule: TaskScheduleConfig::default(),
        parent_task_id: None,
        source_run_id: None,
        source_session_id: None,
        source_turn_id: None,
        source_user_message_id: None,
        prerequisite_task_ids: Vec::new(),
        task_tool_state: TaskToolState::default(),
        plugin_config: Default::default(),
        mcp_config: TaskMcpConfig {
            enabled: false,
            enabled_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "CodeMaintainerWrite".to_string(),
            ],
            external_mcp_config_ids: vec!["external-1".to_string()],
            selected_skill_ids: vec![
                "internal_skill_remotion".to_string(),
                "revoked-skill".to_string(),
            ],
            ..TaskMcpConfig::default()
        },
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    }
}

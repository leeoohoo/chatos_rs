// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{
    now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolState,
};
use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions, McpRuntime, ResourceMetadata, ResourceSecurity,
    SkillContent, SkillInstallationRecord, SkillRecord,
};

fn resolved_mcp(
    id: &str,
    runtime_kind: &str,
    builtin_kind: Option<&str>,
    required: bool,
    available: bool,
) -> ResolvedMcp {
    ResolvedMcp {
        resource: PluginMcpRecord {
            id: id.to_string(),
            owner_user_id: "owner-1".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: id.to_string(),
            display_name: id.to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: runtime_kind.to_string(),
                system_key: (runtime_kind == chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND)
                    .then(|| builtin_kind.map(ToOwned::to_owned))
                    .flatten(),
                builtin_kind: (runtime_kind == BUILTIN_RUNTIME_KIND)
                    .then(|| builtin_kind.map(ToOwned::to_owned))
                    .flatten(),
                url: (runtime_kind == "http").then(|| "http://127.0.0.1/mcp".to_string()),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{id}"),
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            binding_scope: if required {
                "system_required".to_string()
            } else {
                "global_default".to_string()
            },
            owner_user_id: None,
            resource_kind: "mcp".to_string(),
            resource_id: id.to_string(),
            enabled: true,
            required,
            priority: 0,
            conditions: BindingConditions::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available,
        status: if available { "available" } else { "offline" }.to_string(),
        reason: (!available).then(|| "offline".to_string()),
    }
}

fn resolved_skill(id: &str, required: bool, available: bool) -> ResolvedSkill {
    ResolvedSkill {
        resource: SkillRecord {
            id: id.to_string(),
            owner_user_id: "system".to_string(),
            owner_kind: "admin".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "admin_created".to_string(),
            name: "remotion-best-practices".to_string(),
            display_name: "Remotion Best Practices".to_string(),
            description: Some("Local prompt-only Skill".to_string()),
            enabled: true,
            content: SkillContent {
                kind: "local_connector_bundle".to_string(),
                bundle_id: Some("chatos.internal.remotion-best-practices".to_string()),
                bundle_version: Some("1.0.0".to_string()),
                bundle_hash: Some("bundle-hash-1".to_string()),
                entrypoint_kind: Some("prompt_only".to_string()),
                ..SkillContent::default()
            },
            metadata: ResourceMetadata::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{id}"),
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            binding_scope: if required {
                "system_required".to_string()
            } else {
                "global_default".to_string()
            },
            owner_user_id: None,
            resource_kind: "skill".to_string(),
            resource_id: id.to_string(),
            enabled: true,
            required,
            priority: 0,
            conditions: BindingConditions::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available,
        status: if available { "available" } else { "offline" }.to_string(),
        reason: (!available).then(|| "offline".to_string()),
        installation: available.then(|| SkillInstallationRecord {
            id: format!("owner-1:device-1:{id}"),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            skill_id: id.to_string(),
            bundle_id: "chatos.internal.remotion-best-practices".to_string(),
            version: "1.0.0".to_string(),
            bundle_hash: "bundle-hash-1".to_string(),
            platform: "macos-arm64".to_string(),
            status: "available".to_string(),
            dependency_status: "available".to_string(),
            last_error: None,
            last_checked_at: "now".to_string(),
        }),
    }
}

fn policy() -> TaskRunnerCapabilityPolicy {
    TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![
            resolved_mcp(
                "task-manager",
                BUILTIN_RUNTIME_KIND,
                Some("TaskManager"),
                true,
                true,
            ),
            resolved_mcp(
                "ask-user",
                BUILTIN_RUNTIME_KIND,
                Some("AskUser"),
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
        local_connector_requirements: Vec::new(),
    })
    .expect("policy")
}

fn task() -> TaskRecord {
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
        mcp_config: TaskMcpConfig {
            enabled: false,
            enabled_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "CodeMaintainerWrite".to_string(),
            ],
            external_mcp_config_ids: vec!["external-1".to_string(), "revoked".to_string()],
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

#[test]
fn ai_selectable_sets_exclude_required_and_unavailable_capabilities() {
    let policy = policy();
    assert_eq!(
        policy.selectable_builtin_kind_names(),
        vec!["CodeMaintainerRead".to_string()]
    );
    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec!["external-1".to_string()]
    );
    assert!(policy.selectable_skill_ids().is_empty());
}

#[test]
fn runtime_injects_required_and_intersects_saved_optional_selection() {
    let mut task = task();
    policy().apply_to_task(&mut task).expect("apply policy");
    assert!(task.mcp_config.enabled);
    assert_eq!(
        task.mcp_config.enabled_builtin_kinds,
        vec![
            "CodeMaintainerRead".to_string(),
            "TaskManager".to_string(),
            "AskUser".to_string(),
        ]
    );
    assert_eq!(
        task.mcp_config.external_mcp_config_ids,
        vec!["external-1".to_string()]
    );
    assert!(task.mcp_config.selected_skill_ids.is_empty());
    let snapshots = policy().skill_snapshots(&task).expect("skill snapshots");
    assert!(snapshots.is_empty());
}

#[test]
fn planning_policy_injects_its_non_mutating_builtin_allowlist() {
    let mut policy = policy();
    policy.capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    for item in &mut policy.capabilities.mcps {
        item.binding.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
        if item.resource.id == "external-1" {
            item.resource.security.allow_writes = Some(true);
        }
        if item.resource.id == "write" {
            item.available = true;
            item.status = "available".to_string();
            item.reason = None;
        }
    }
    let mut task = task();
    task.task_profile = crate::models::TASK_PROFILE_CHATOS_PLAN.to_string();
    task.mcp_config.requires_execution = false;
    task.mcp_config.enabled_builtin_kinds.clear();

    policy.apply_to_task(&mut task).expect("apply plan policy");

    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerRead".to_string()));
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"TaskManager".to_string()));
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"AskUser".to_string()));
    assert!(!task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerWrite".to_string()));
    assert!(!task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"TerminalController".to_string()));
    assert!(policy.selectable_external_mcp_ids().is_empty());
}

#[test]
fn planning_policy_rejects_required_mutating_tools() {
    let mut capabilities = policy().capabilities;
    capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    let write = capabilities
        .mcps
        .iter_mut()
        .find(|item| item.resource.id == "write")
        .expect("write capability");
    write.binding.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    write.binding.required = true;
    write.available = true;
    write.status = "available".to_string();
    write.reason = None;

    let error = TaskRunnerCapabilityPolicy::new(capabilities)
        .expect_err("planning policy must reject mutating required tools");
    assert!(error.contains("cannot be required for task_runner_plan_phase"));
}

#[test]
fn policy_rejects_write_when_read_is_not_configured_for_the_same_agent() {
    let mut capabilities = policy().capabilities;
    capabilities
        .mcps
        .retain(|item| plugin_builtin_kind(item) != Some(BuiltinMcpKind::CodeMaintainerRead));
    let write = capabilities
        .mcps
        .iter_mut()
        .find(|item| plugin_builtin_kind(item) == Some(BuiltinMcpKind::CodeMaintainerWrite))
        .expect("write capability");
    write.available = true;
    write.status = "available".to_string();
    write.reason = None;

    let error = TaskRunnerCapabilityPolicy::new(capabilities)
        .expect_err("write-only Plugin configuration must fail closed");
    assert!(error.contains("enables CodeMaintainerWrite without CodeMaintainerRead"));
}

#[test]
fn disabled_task_runner_agent_fails_closed() {
    let mut capabilities = policy().capabilities;
    capabilities.agent_enabled = false;
    let error =
        TaskRunnerCapabilityPolicy::new(capabilities).expect_err("disabled Agent must not execute");
    assert!(error.contains("disabled by Plugin Management"));
}

#[test]
fn write_validation_rejects_required_and_unavailable_selection() {
    let mut config = TaskMcpConfig {
        enabled_builtin_kinds: vec!["TaskManager".to_string()],
        ..TaskMcpConfig::default()
    };
    assert!(policy().validate_optional_config(&config).is_err());
    config.enabled_builtin_kinds = vec!["CodeMaintainerWrite".to_string()];
    assert!(policy().validate_optional_config(&config).is_err());
}

#[test]
fn cloud_policy_excludes_local_connector_mcps() {
    let mut local = resolved_mcp("local-user", "local_connector_http", None, false, true);
    local.resource.source_kind = LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND.to_string();
    let cloud = resolved_mcp("cloud-http", "http", None, false, true);
    let policy = TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-local".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![local, cloud],
        skills: Vec::new(),
        local_connector_requirements: Vec::new(),
    })
    .expect("policy");

    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec!["cloud-http".to_string()]
    );
}

#[test]
fn unified_service_system_mcp_is_selected_as_a_task_runner_backend() {
    let system = resolved_mcp(
        chatos_plugin_management_sdk::PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
        chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND,
        Some("project_runtime_environment"),
        false,
        true,
    );
    let policy = TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-system".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![system],
        skills: Vec::new(),
        local_connector_requirements: Vec::new(),
    })
    .expect("policy");

    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec![chatos_plugin_management_sdk::PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID.to_string()]
    );
}

#[test]
fn user_created_cloud_mcp_is_allowed_and_local_connector_mcp_is_rejected() {
    for runtime_kind in ["http", "stdio_cloud"] {
        let mut item = resolved_mcp("user-cloud-mcp", runtime_kind, None, false, true);
        item.resource.source_kind = "user_created".to_string();
        item.resource.owner_kind = "user".to_string();
        validate_cloud_external_mcp_runtime(&item)
            .expect("user-created cloud MCP should remain cloud-runnable");
    }

    let local = resolved_mcp("local-mcp", "local_connector_stdio", None, false, true);
    let err = validate_cloud_external_mcp_runtime(&local)
        .expect_err("Local Connector MCP must be rejected by cloud policy");
    assert!(err.contains("unavailable in cloud Task Runner"));
}

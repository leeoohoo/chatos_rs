// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{
    agent_prompt_checksum, AgentBindingRecord, AgentPromptBundle, AgentPromptVendor,
    BindingConditions, McpRecord, McpRuntime, ResolvedAgentCapabilities, ResolvedAgentPrompt,
    ResolvedMcp, ResourceMetadata, ResourceSecurity, SystemAgentKey,
};

use crate::local_runtime::storage::LocalDatabase;

pub(in crate::local_runtime) async fn seed_chat_capabilities(
    database: &LocalDatabase,
    owner_user_id: &str,
) -> anyhow::Result<()> {
    database
        .install_agent_prompt_bundle("https://cloud.example.invalid", &test_agent_prompt_bundle())
        .await?;
    for agent_key in [
        SystemAgentKey::ChatosConversationAgent,
        SystemAgentKey::ChatosPlanningAgent,
    ] {
        database
            .save_capability_snapshot(&capabilities(
                owner_user_id,
                agent_key,
                vec![resolved_task_runner(agent_key)],
            ))
            .await?;
    }
    let execution_mcps = [
        BuiltinMcpKind::CodeMaintainerRead,
        BuiltinMcpKind::CodeMaintainerWrite,
        BuiltinMcpKind::TerminalController,
        BuiltinMcpKind::BrowserTools,
        BuiltinMcpKind::TaskManager,
        BuiltinMcpKind::ProjectManagement,
        BuiltinMcpKind::AskUser,
    ]
    .into_iter()
    .map(|kind| {
        resolved_builtin(
            SystemAgentKey::TaskRunnerRunPhase,
            kind,
            matches!(kind, BuiltinMcpKind::TaskManager | BuiltinMcpKind::AskUser),
        )
    })
    .collect();
    database
        .save_capability_snapshot(&capabilities(
            owner_user_id,
            SystemAgentKey::TaskRunnerRunPhase,
            execution_mcps,
        ))
        .await?;
    let planning_mcps = [
        BuiltinMcpKind::CodeMaintainerRead,
        BuiltinMcpKind::TaskManager,
        BuiltinMcpKind::ProjectManagement,
        BuiltinMcpKind::BrowserTools,
        BuiltinMcpKind::AskUser,
    ]
    .into_iter()
    .map(|kind| {
        resolved_builtin(
            SystemAgentKey::TaskRunnerPlanPhase,
            kind,
            matches!(kind, BuiltinMcpKind::TaskManager | BuiltinMcpKind::AskUser),
        )
    })
    .collect();
    database
        .save_capability_snapshot(&capabilities(
            owner_user_id,
            SystemAgentKey::TaskRunnerPlanPhase,
            planning_mcps,
        ))
        .await
}

pub(in crate::local_runtime) async fn grant_required_builtin(
    database: &LocalDatabase,
    owner_user_id: &str,
    agent_key: SystemAgentKey,
    kind: BuiltinMcpKind,
) -> anyhow::Result<()> {
    database
        .save_capability_snapshot(&capabilities(
            owner_user_id,
            agent_key,
            vec![resolved_builtin(agent_key, kind, true)],
        ))
        .await
}

fn test_agent_prompt_bundle() -> AgentPromptBundle {
    let prompts = SystemAgentKey::ALL
        .into_iter()
        .flat_map(|agent_key| {
            AgentPromptVendor::ALL.into_iter().map(move |vendor| {
                let content = format!("{} {vendor} test prompt", agent_key.as_str());
                ResolvedAgentPrompt {
                    agent_key: agent_key.as_str().to_string(),
                    vendor,
                    checksum: agent_prompt_checksum(content.as_str()),
                    content,
                    revision: 1,
                    published_at: "2026-07-16T00:00:00Z".to_string(),
                }
            })
        })
        .collect();
    AgentPromptBundle {
        bundle_version: 1,
        updated_at: "2026-07-16T00:00:00Z".to_string(),
        prompts,
    }
}

fn capabilities(
    owner_user_id: &str,
    agent_key: SystemAgentKey,
    mcps: Vec<ResolvedMcp>,
) -> ResolvedAgentCapabilities {
    ResolvedAgentCapabilities {
        agent_key: agent_key.as_str().to_string(),
        owner_user_id: owner_user_id.to_string(),
        policy_revision: "test-policy".to_string(),
        generated_at: "2026-07-15T00:00:00Z".to_string(),
        agent_enabled: true,
        mcps,
        skills: Vec::new(),
        local_connector_requirements: Vec::new(),
    }
}

fn resolved_builtin(
    agent_key: SystemAgentKey,
    kind: BuiltinMcpKind,
    required: bool,
) -> ResolvedMcp {
    let resource_id = kind
        .config_id()
        .map(str::to_string)
        .unwrap_or_else(|| kind.kind_name().to_string());
    ResolvedMcp {
        resource: McpRecord {
            id: resource_id.clone(),
            owner_user_id: "system".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: kind.server_name().to_string(),
            display_name: kind.kind_name().to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: "builtin".to_string(),
                builtin_kind: Some(kind.kind_name().to_string()),
                server_name: Some(kind.server_name().to_string()),
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
            id: format!("binding-{resource_id}"),
            agent_key: agent_key.as_str().to_string(),
            binding_scope: if required {
                "system_required".to_string()
            } else {
                "global_default".to_string()
            },
            owner_user_id: None,
            resource_kind: "mcp".to_string(),
            resource_id,
            enabled: true,
            required,
            priority: 0,
            conditions: BindingConditions::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available: true,
        status: "available".to_string(),
        reason: None,
    }
}

fn resolved_task_runner(agent_key: SystemAgentKey) -> ResolvedMcp {
    let descriptor = chatos_mcp::system_mcp_descriptor(
        chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService,
    );
    ResolvedMcp {
        resource: McpRecord {
            id: descriptor.resource_id.to_string(),
            owner_user_id: "system".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: descriptor.server_name.to_string(),
            display_name: descriptor.display_name.to_string(),
            description: Some(descriptor.description.to_string()),
            enabled: true,
            runtime: McpRuntime {
                kind: "system".to_string(),
                system_key: Some(descriptor.key.as_str().to_string()),
                server_name: Some(descriptor.server_name.to_string()),
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
            id: format!("binding-{}", descriptor.resource_id),
            agent_key: agent_key.as_str().to_string(),
            binding_scope: "system_required".to_string(),
            owner_user_id: None,
            resource_kind: "mcp".to_string(),
            resource_id: descriptor.resource_id.to_string(),
            enabled: true,
            required: true,
            priority: 0,
            conditions: BindingConditions::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available: true,
        status: "available".to_string(),
        reason: None,
    }
}

#[tokio::test]
async fn conversation_and_planning_agents_do_not_inherit_task_runner_file_permissions() {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use uuid::Uuid;

    use crate::local_runtime::capabilities::resolve_local_chat_capabilities;
    use crate::local_runtime::storage::LocalRuntimeSettingsRecord;
    use crate::relay::RelayRequest;
    use crate::LocalState;

    let root = std::env::temp_dir().join(format!("chatos-capability-isolation-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local capability database");
    seed_chat_capabilities(&database, "user-1")
        .await
        .expect("seed isolated Agent capabilities");
    let settings = LocalRuntimeSettingsRecord {
        session_id: "session-1".to_string(),
        selected_model_id: None,
        selected_model_name: None,
        selected_thinking_level: None,
        workspace_root: None,
        reasoning_enabled: false,
        plan_mode_enabled: false,
        mcp_enabled: true,
        enabled_mcp_ids_json: "[]".to_string(),
        selected_skill_ids_json: "[]".to_string(),
        auto_create_task: false,
        memory_auto_summary_enabled: false,
        memory_summary_message_threshold: 0,
        memory_summary_character_threshold: 0,
        memory_recall_limit: 0,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    let request = RelayRequest {
        _message_type: "test".to_string(),
        request_id: "request-1".to_string(),
        owner_user_id: Some("user-1".to_string()),
        device_id: Some("device-1".to_string()),
        workspace_id: "workspace-1".to_string(),
        method: None,
        path: None,
        headers: BTreeMap::new(),
        body: Value::Null,
    };
    for agent_key in [
        SystemAgentKey::ChatosConversationAgent,
        SystemAgentKey::ChatosPlanningAgent,
    ] {
        let resolved = resolve_local_chat_capabilities(
            &database,
            "user-1",
            &settings,
            &LocalState::default(),
            &request,
            agent_key,
            false,
            Vec::new(),
        )
        .await
        .expect("resolve primary Agent capability");
        assert!(resolved.builtin_kinds.is_empty());
        assert_eq!(
            resolved.host_system_mcps,
            vec![chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService]
        );
    }

    let task_runner = resolve_local_chat_capabilities(
        &database,
        "user-1",
        &settings,
        &LocalState::default(),
        &request,
        SystemAgentKey::TaskRunnerRunPhase,
        true,
        Vec::new(),
    )
    .await
    .expect("resolve Task Runner capability");
    assert!(task_runner
        .builtin_kinds
        .contains(&BuiltinMcpKind::CodeMaintainerRead));

    database.close().await;
    std::fs::remove_dir_all(root).expect("cleanup local capability database");
}

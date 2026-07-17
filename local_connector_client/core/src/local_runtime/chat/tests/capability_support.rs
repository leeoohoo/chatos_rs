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
            .save_capability_snapshot(&capabilities(owner_user_id, agent_key, Vec::new()))
            .await?;
    }
    let mcps = [
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
            kind,
            matches!(kind, BuiltinMcpKind::TaskManager | BuiltinMcpKind::AskUser),
        )
    })
    .collect();
    database
        .save_capability_snapshot(&capabilities(
            owner_user_id,
            SystemAgentKey::TaskRunnerRunPhase,
            mcps,
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

fn resolved_builtin(kind: BuiltinMcpKind, required: bool) -> ResolvedMcp {
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
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
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

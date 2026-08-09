// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{ChatosAgentProfile, CHATOS_PLAN_TASK_PROFILE};

use super::policy::merge_optional_system_prompts;

#[test]
fn plan_mode_is_a_program_routed_task_profile_not_a_second_chatos_agent() {
    assert_eq!(
        ChatosAgentProfile::from_flags(false, false).key(),
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent
    );
    assert_eq!(
        ChatosAgentProfile::from_flags(true, false).key(),
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent
    );
    assert_eq!(
        ChatosAgentProfile::from_flags(true, false).task_runner_task_profile(),
        Some(CHATOS_PLAN_TASK_PROFILE)
    );
    assert_eq!(
        ChatosAgentProfile::from_flags(false, true).key(),
        chatos_plugin_management_sdk::SystemAgentKey::ProjectRequirementExecutionPlannerAgent
    );
}

#[test]
fn provider_skills_are_composed_into_the_mcp_system_context() {
    let mut metadata = chatos_plugin_management_sdk::ResourceMetadata::default();
    metadata.extra.insert(
        "provider_skills".to_string(),
        serde_json::json!([{
            "id": "task_runner_usage",
            "name": "Task Runner Usage",
            "description": "Create durable background tasks.",
            "instructions": "Use the business tools exposed for this Runtime Session. Tool ownership and routing are resolved by the program."
        }]),
    );

    let mcp = chatos_plugin_management_sdk::McpRecord {
        id: "task-runner".to_string(),
        owner_user_id: "owner".to_string(),
        owner_kind: "system".to_string(),
        visibility: "system_private".to_string(),
        source_kind: "system_seed".to_string(),
        name: "task_runner_service".to_string(),
        display_name: "Task Runner".to_string(),
        description: None,
        enabled: true,
        runtime: chatos_plugin_management_sdk::McpRuntime {
            server_name: Some("task_runner_service".to_string()),
            ..Default::default()
        },
        security: Default::default(),
        metadata,
        plugin_component: Default::default(),
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    let prompt =
        chatos_plugin_management_sdk::compose_mcp_provider_skills_prompt([&mcp], Some("zh-CN"))
            .expect("provider prompt");

    assert!(prompt.contains("Tool Usage Instructions"));
    assert!(!prompt.contains("Provider"));
    assert!(prompt.contains("task_runner_service"));
    assert!(prompt.contains("business tools exposed for this Runtime Session"));
    assert!(prompt.contains("routing are resolved by the program"));
    assert!(!prompt.contains("list_available_plugins"));
    assert!(!prompt.contains("selected_plugins"));
}

#[test]
fn provider_skill_prompt_is_appended_to_existing_contact_prompt() {
    let merged = merge_optional_system_prompts(
        Some("contact instructions".to_string()),
        Some("provider instructions".to_string()),
    )
    .expect("merged prompt");

    assert_eq!(merged, "contact instructions\n\nprovider instructions");
}

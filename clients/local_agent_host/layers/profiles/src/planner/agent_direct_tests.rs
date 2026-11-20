use super::*;

fn input(permission_ids: &[&str]) -> Value {
    json!({
        "agent_execution_context": {
            "room_id": "room-1",
            "agent_id": "agent-1",
            "agent_name": "工程 Agent",
            "role_prompt": "负责工程实现。",
            "profession_key": "software_engineer",
            "profession_label": "软件工程师",
            "profession_skill_ref": "AS-profession-software_engineer-123456789abc",
            "profession_skill_sha256": "a".repeat(64),
            "profession_skill_markdown": "# 工作流\n先核实，再实现。",
            "default_plugin_ids": [],
            "default_skill_ids": permission_ids,
        }
    })
}

#[test]
fn bound_agent_identity_and_skill_become_model_instructions() {
    let instructions = agent_direct_instructions(&input(&[])).unwrap().unwrap();
    assert!(instructions.contains("工程 Agent"));
    assert!(instructions.contains("负责工程实现"));
    assert!(instructions.contains("AS-profession-software_engineer"));
    assert!(instructions.contains("先核实，再实现"));
}

#[test]
fn project_task_tools_require_the_persisted_agent_permission() {
    let tools = vec![
        json!({"name": "create_task"}),
        json!({"name": "agent_skill_activate"}),
        json!({"name": "read_conversation_attachment"}),
    ];
    let restricted = scope_agent_direct_tools(tools.clone(), &input(&[]));
    assert!(!restricted.iter().any(|tool| tool["name"] == "create_task"));
    assert!(restricted
        .iter()
        .any(|tool| tool["name"] == "agent_skill_activate"));

    let allowed = scope_agent_direct_tools(tools, &input(&[AGENT_PROJECT_PERMISSION]));
    assert!(allowed.iter().any(|tool| tool["name"] == "create_task"));
}

#[test]
fn team_agent_identity_includes_scoped_role_without_claiming_manager_authority() {
    let mut team_input = input(&[AGENT_PROJECT_PERMISSION]);
    let context = team_input["agent_execution_context"]
        .as_object_mut()
        .unwrap();
    context.insert("schema_version".into(), json!(2));
    context.insert("project_id".into(), json!("project-1"));
    context.insert("team_name".into(), json!("Alpha 团队"));
    context.insert("team_goal".into(), json!("交付 Alpha"));
    context.insert("team_role".into(), json!("前端工程师"));
    context.insert("team_responsibility".into(), json!("负责 Windows UI"));
    context.insert("is_project_manager".into(), json!(false));

    let instructions = agent_direct_instructions(&team_input).unwrap().unwrap();
    assert!(instructions.contains("Bound ChatOS Team Agent Identity"));
    assert!(instructions.contains("Current team role: 前端工程师"));
    assert!(instructions.contains("Project manager: false"));
    assert!(instructions.contains("Do not claim project-manager authority"));
    assert!(!instructions.contains("private conversation"));
}

#[test]
fn team_asset_tools_are_visible_only_to_the_bound_team_authority() {
    let tools = vec![
        json!({"name": "team_asset_list"}),
        json!({"name": "team_asset_get"}),
        json!({"name": "team_asset_create"}),
        json!({"name": "team_asset_update"}),
        json!({"name": "team_asset_archive"}),
    ];
    assert!(scope_agent_direct_tools(tools.clone(), &json!({})).is_empty());

    let mut member = input(&[]);
    member["agent_execution_context"]["schema_version"] = json!(2);
    member["agent_execution_context"]["is_project_manager"] = json!(false);
    let member_tools = scope_agent_direct_tools(tools.clone(), &member);
    assert_eq!(member_tools.len(), 2);
    assert!(member_tools.iter().all(|tool| matches!(
        tool["name"].as_str(),
        Some("team_asset_list" | "team_asset_get")
    )));

    member["agent_execution_context"]["is_project_manager"] = json!(true);
    assert_eq!(scope_agent_direct_tools(tools, &member).len(), 5);
}

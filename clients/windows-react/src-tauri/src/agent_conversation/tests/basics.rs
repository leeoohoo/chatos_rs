use super::*;

#[test]
fn team_executor_conversation_is_isolated_from_the_unique_project_resource() {
    let detail = LocalConversationDetail {
        conversation: chatos_local_agent_protocol::LocalConversationRecord {
            conversation_id: "agent-team-executor-delivery-1".into(),
            owner_user_id: "alice".into(),
            title: "Team · Todo".into(),
            resource: None,
            version: 1,
            created_at_unix_ms: 100,
            updated_at_unix_ms: 100,
        },
        turns: vec![],
        messages: vec![],
        attachments: vec![],
    };
    validate_team_executor_host_conversation(&detail, "alice", "agent-team-executor-delivery-1")
        .unwrap();

    let mut project_bound = detail;
    project_bound.conversation.resource = Some(LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Project,
        resource_id: "project-1".into(),
    });
    assert!(validate_team_executor_host_conversation(
        &project_bound,
        "alice",
        "agent-team-executor-delivery-1",
    )
    .is_err());
}

#[test]
fn direct_room_is_idempotent_and_binds_exactly_one_active_agent() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");

    let first = store.open_human_agent_direct("alice", "agent-1").unwrap();
    let second = store.open_human_agent_direct("alice", "agent-1").unwrap();
    assert_eq!(first, second);
    assert_eq!(first.agent_id, "agent-1");

    let connection = Connection::open(store.database()).unwrap();
    let count: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = 'alice' AND room_id = ?1 AND status = 'active'",
            params![first.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn direct_room_rejects_foreign_archived_and_malformed_agents() {
    let (_directory, store) = store();
    insert_agent(&store, "bob", "foreign", "active");
    insert_agent(&store, "alice", "archived", "archived");

    assert!(store.open_human_agent_direct("alice", "foreign").is_err());
    assert!(store.open_human_agent_direct("alice", "archived").is_err());
    assert!(store.open_human_agent_direct("alice", "../agent").is_err());
}

#[test]
fn execution_binding_is_loaded_only_from_the_exact_room_member_and_profile() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let room = store.open_human_agent_direct("alice", "agent-1").unwrap();

    let binding = store.load_execution_binding("alice", &room.id).unwrap();
    assert_eq!(binding.room_id, room.id);
    assert_eq!(binding.agent_id, "agent-1");
    assert_eq!(binding.model_config_id, "model-1");
    assert_eq!(binding.thinking_level.as_deref(), Some("high"));
    assert_eq!(binding.profession_key, "software_engineer");
    assert_eq!(binding.default_skill_ids, vec!["local.project.list"]);
    assert!(store.load_execution_binding("bob", &room.id).is_err());
}

#[test]
fn project_teams_are_account_scoped_and_unique_per_active_project() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    insert_agent(&store, "bob", "agent-2", "active");
    let created = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    assert_eq!(created.project_id, "project-1");
    assert_eq!(created.member_count, 0);
    assert_eq!(
        store.list_project_teams("alice").unwrap(),
        vec![created.clone()]
    );
    assert!(store
        .load_project_team("alice", &created.id)
        .unwrap()
        .members
        .is_empty());
    let invited = store
        .invite_agent_to_project_team("alice", &created.id, "agent-1")
        .unwrap();
    assert_eq!(invited.team.member_count, 1);
    assert_eq!(invited.team.default_agent_id.as_deref(), Some("agent-1"));
    assert_eq!(invited.members.len(), 1);
    assert_eq!(invited.members[0].agent_id, "agent-1");
    assert_eq!(invited.members[0].role, "测试 Agent");
    assert!(store
        .invite_agent_to_project_team("alice", &created.id, "agent-1")
        .is_err());
    assert!(store
        .invite_agent_to_project_team("alice", &created.id, "agent-2")
        .is_err());
    assert!(store.list_project_teams("bob").unwrap().is_empty());
    assert!(store
        .create_project_team("alice", "project-1", "重复团队", "")
        .is_err());
    assert!(store
        .create_project_team("bob", "project-1", "Bob 团队", "")
        .is_ok());
}

#[test]
fn project_team_member_management_matches_macos_invariants() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    insert_agent(&store, "alice", "manager", "active");
    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
            [],
        )
        .unwrap();
    drop(connection);
    let created = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &created.id, "agent-1")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &created.id, "manager")
        .unwrap();

    let updated = store
        .update_project_team_member(
            "alice",
            &created.id,
            "agent-1",
            "前端工程师",
            "负责 Windows UI",
        )
        .unwrap();
    assert_eq!(updated.members[0].role, "前端工程师");
    assert_eq!(updated.members[0].responsibility, "负责 Windows UI");
    assert!(store
        .set_project_team_manager("alice", &created.id, "agent-1")
        .is_err());

    let managed = store
        .set_project_team_manager("alice", &created.id, "manager")
        .unwrap();
    assert_eq!(
        managed.team.project_manager_agent_id.as_deref(),
        Some("manager")
    );
    assert!(store
        .remove_agent_from_project_team("alice", &created.id, "manager")
        .is_err());

    let defaulted = store
        .set_project_team_default_agent("alice", &created.id, "manager")
        .unwrap();
    assert_eq!(defaulted.team.default_agent_id.as_deref(), Some("manager"));
    let removed = store
        .remove_agent_from_project_team("alice", &created.id, "agent-1")
        .unwrap();
    assert_eq!(removed.team.member_count, 1);
    assert_eq!(removed.members[0].agent_id, "manager");
    assert!(store
        .update_project_team_member("bob", &created.id, "manager", "项目经理", "负责交付",)
        .is_err());
}

#[test]
fn removing_the_default_team_agent_selects_the_next_active_member() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    insert_agent(&store, "alice", "agent-2", "active");
    let created = store
        .create_project_team("alice", "project-1", "Alpha 团队", "")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &created.id, "agent-1")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &created.id, "agent-2")
        .unwrap();

    let removed = store
        .remove_agent_from_project_team("alice", &created.id, "agent-1")
        .unwrap();
    assert_eq!(removed.team.default_agent_id.as_deref(), Some("agent-2"));
    assert_eq!(removed.team.member_count, 1);
}

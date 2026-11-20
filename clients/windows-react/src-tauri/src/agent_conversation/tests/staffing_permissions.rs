use super::*;

#[test]
fn direct_staffing_creation_is_permission_bound_idempotent_and_profile_only() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "manager", "active");
    let room = store.open_human_agent_direct("alice", "manager").unwrap();
    let binding = store.load_execution_binding("alice", &room.id).unwrap();
    store
        .record_direct_delivery(
            "alice",
            &binding,
            "delivery-staffing-direct",
            "message-staffing-direct",
            "turn-staffing-direct",
            "run-staffing-direct",
            "请评估人员配置",
            10,
        )
        .unwrap();

    let unauthorized = store.create_agent_creation_proposal(
        "alice",
        "run-staffing-direct",
        "proposal-direct-create",
        "后端工程师",
        "后端工程师",
        "负责服务端交付",
        "完成服务端任务",
        None,
        "backend_engineer",
        "存在长期职责缺口",
        11,
    );
    assert_eq!(
        unauthorized.unwrap_err(),
        "The current Agent does not have staff-management permission."
    );
    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "UPDATE local_agent_profiles SET default_skill_ids_json = \
                 '[\"agent.staff.hire\",\"agent.staff.terminate\"]' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
            [],
        )
        .unwrap();
    drop(connection);

    for _ in 0..2 {
        store
            .create_agent_creation_proposal(
                "alice",
                "run-staffing-direct",
                "proposal-direct-create",
                "后端工程师",
                "后端工程师",
                "负责服务端交付",
                "完成服务端任务",
                None,
                "backend_engineer",
                "存在长期职责缺口",
                12,
            )
            .unwrap();
    }
    assert!(store
        .create_agent_creation_proposal(
            "alice",
            "run-staffing-direct",
            "proposal-direct-create",
            "不同名称",
            "后端工程师",
            "负责服务端交付",
            "完成服务端任务",
            None,
            "backend_engineer",
            "存在长期职责缺口",
            12,
        )
        .unwrap_err()
        .contains("reused with different input"));
    let proposals = store
        .list_agent_staffing_proposals("alice", &room.id)
        .unwrap();
    assert_eq!(proposals.len(), 1);
    let resolution = store
        .resolve_agent_staffing_proposal(
            "alice",
            &room.id,
            &proposals[0].id,
            "agentCreation",
            true,
            13,
        )
        .unwrap();
    assert_eq!(resolution.status, "approved");
    let created_agent_id = resolution.created_agent_id.unwrap();
    let connection = Connection::open(store.database()).unwrap();
    let created: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_agent_profiles WHERE owner_user_id = 'alice' \
                 AND id = ?1 AND status = 'active')",
            params![created_agent_id],
            |row| row.get(0),
        )
        .unwrap();
    let direct_member_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_room_members WHERE owner_user_id = 'alice' \
                 AND room_id = ?1 AND status = 'active'",
            params![room.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(created);
    assert_eq!(direct_member_count, 1);
}

#[test]
fn staffing_tools_all_require_permission_and_manager_removal_requires_handoff() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "proposer", "active");
    insert_agent(&store, "alice", "manager", "active");
    Connection::open(store.database())
        .unwrap()
        .execute(
            "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
            [],
        )
        .unwrap();
    let team = store
        .create_project_team("alice", "project-1", "项目团队", "完成项目")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "proposer")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "manager")
        .unwrap();
    store
        .set_project_team_manager("alice", &team.id, "manager")
        .unwrap();
    store
        .queue_project_team_message("alice", &team.id, "请评估人员配置", &["proposer".into()])
        .unwrap();
    let pending = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery(
            "alice",
            &pending,
            "turn-staffing-permission",
            "run-staffing-permission",
            10,
        )
        .unwrap();
    for result in [
        store
            .create_agent_creation_proposal(
                "alice",
                "run-staffing-permission",
                "unauthorized-create",
                "新成员",
                "工程师",
                "负责交付",
                "完成任务",
                None,
                "software_engineer",
                "补充人力",
                11,
            )
            .map(|_| ()),
        store
            .create_agent_membership_proposal(
                "alice",
                "run-staffing-permission",
                "unauthorized-membership",
                "team_ref_not_resolved",
                "agent_ref_not_resolved",
                "工程师",
                "负责交付",
                11,
            )
            .map(|_| ()),
        store
            .create_agent_removal_proposal(
                "alice",
                "run-staffing-permission",
                "unauthorized-removal",
                "agent_ref_not_resolved",
                "职责调整",
                "完成交接",
                11,
            )
            .map(|_| ()),
    ] {
        assert_eq!(
            result.unwrap_err(),
            "The current Agent does not have staff-management permission."
        );
    }

    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "UPDATE local_agent_profiles SET default_skill_ids_json = \
                 '[\"agent.staff.hire\",\"agent.staff.terminate\"]' \
                 WHERE owner_user_id = 'alice' AND id = 'proposer'",
            [],
        )
        .unwrap();
    drop(connection);
    let (_, workspace) = store
        .agent_workspace_snapshot("alice", "run-staffing-permission")
        .unwrap();
    let manager_reference = workspace["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["name"] == "测试 Agent" && agent["is_current_agent"] == false)
        .unwrap()["agent_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    store
        .create_agent_removal_proposal(
            "alice",
            "run-staffing-permission",
            "manager-removal",
            &manager_reference,
            "职责调整",
            "先完成负责人交接",
            12,
        )
        .unwrap();
    let proposal = store
        .list_agent_staffing_proposals("alice", &team.id)
        .unwrap()
        .remove(0);
    assert_eq!(
        store
            .resolve_agent_staffing_proposal(
                "alice",
                &team.id,
                &proposal.id,
                "agentRemoval",
                true,
                13,
            )
            .unwrap_err(),
        "请先指定新的项目经理，再批准移出当前项目经理。"
    );
    assert!(store
        .load_project_team("alice", &team.id)
        .unwrap()
        .members
        .iter()
        .any(|member| member.agent_id == "manager"));
}

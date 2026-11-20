use super::*;

#[test]
fn staffing_proposals_require_human_resolution_and_notify_the_proposer() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "manager", "active");
    insert_agent(&store, "alice", "target", "active");
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
    let source = store
        .create_project_team("alice", "project-1", "来源团队", "管理团队成员")
        .unwrap();
    let target_team = store
        .create_project_team("alice", "project-2", "目标团队", "接收现有 Agent")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &source.id, "manager")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &source.id, "target")
        .unwrap();
    let queued = store
        .queue_project_team_message("alice", &source.id, "请评估团队配置", &["manager".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &source.id)
        .unwrap()
        .unwrap();
    assert_eq!(delivery.delivery_id, queued.deliveries[0].delivery_id);
    store
        .claim_project_team_delivery("alice", &delivery, "turn-staffing", "run-staffing", 10)
        .unwrap();
    let (_, workspace) = store
        .agent_workspace_snapshot("alice", "run-staffing")
        .unwrap();
    let target_agent_reference = workspace["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["is_current_agent"] == false)
        .unwrap()["agent_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let target_team_reference = workspace["teams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|team| team["name"] == "目标团队")
        .unwrap()["team_ref"]
        .as_str()
        .unwrap()
        .to_owned();

    let (_, membership_ack) = store
        .create_agent_membership_proposal(
            "alice",
            "run-staffing",
            "proposal-membership",
            &target_team_reference,
            &target_agent_reference,
            "测试工程师",
            "负责验收",
            11,
        )
        .unwrap();
    assert_eq!(membership_ack["status"], "pending");
    assert!(store
        .load_project_team("alice", &target_team.id)
        .unwrap()
        .members
        .is_empty());
    let membership = store
        .list_agent_staffing_proposals("alice", &source.id)
        .unwrap()
        .into_iter()
        .find(|proposal| proposal.kind == "existingAgentMembership")
        .unwrap();
    let membership_resolution = store
        .resolve_agent_staffing_proposal(
            "alice",
            &source.id,
            &membership.id,
            "existingAgentMembership",
            true,
            12,
        )
        .unwrap();
    assert_eq!(membership_resolution.status, "approved");
    assert_eq!(
        store
            .load_project_team("alice", &target_team.id)
            .unwrap()
            .members[0]
            .agent_id,
        "target"
    );

    let (_, creation_ack) = store
        .create_agent_creation_proposal(
            "alice",
            "run-staffing",
            "proposal-creation",
            "新同事",
            "后端工程师",
            "实现服务端能力",
            "严格完成后端任务",
            None,
            "backend_engineer",
            "团队存在长期后端职责缺口",
            13,
        )
        .unwrap();
    assert_eq!(creation_ack["status"], "pending");
    let creation = store
        .list_agent_staffing_proposals("alice", &source.id)
        .unwrap()
        .into_iter()
        .find(|proposal| proposal.kind == "agentCreation")
        .unwrap();
    let creation_resolution = store
        .resolve_agent_staffing_proposal(
            "alice",
            &source.id,
            &creation.id,
            "agentCreation",
            true,
            14,
        )
        .unwrap();
    assert_eq!(creation_resolution.status, "approved");
    let created_agent_id = creation_resolution.created_agent_id.unwrap();
    assert!(store
        .load_project_team("alice", &source.id)
        .unwrap()
        .members
        .iter()
        .any(|member| member.agent_id == created_agent_id));

    let (_, removal_ack) = store
        .create_agent_removal_proposal(
            "alice",
            "run-staffing",
            "proposal-removal",
            &target_agent_reference,
            "当前团队职责已经调整",
            "先完成现有验收交接",
            15,
        )
        .unwrap();
    assert_eq!(removal_ack["status"], "pending");
    let removal = store
        .list_agent_staffing_proposals("alice", &source.id)
        .unwrap()
        .into_iter()
        .find(|proposal| proposal.kind == "agentRemoval")
        .unwrap();
    store
        .resolve_agent_staffing_proposal("alice", &source.id, &removal.id, "agentRemoval", true, 16)
        .unwrap();
    assert!(!store
        .load_project_team("alice", &source.id)
        .unwrap()
        .members
        .iter()
        .any(|member| member.agent_id == "target"));
    assert!(store
        .list_agent_staffing_proposals("alice", &source.id)
        .unwrap()
        .is_empty());
    let connection = Connection::open(store.database()).unwrap();
    let resolution_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_deliveries WHERE owner_user_id = 'alice' \
                 AND room_id = ?1 AND trigger_kind = 'mention' \
                 AND deduplication_key LIKE 'proposal-resolution:%'",
            params![source.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolution_count, 3);
}

#[test]
fn direct_staffing_resolution_runs_without_scheduling_the_human_ui_delivery() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "manager", "active");
    Connection::open(store.database())
        .unwrap()
        .execute(
            "UPDATE local_agent_profiles SET default_skill_ids_json = \
                 '[\"agent.staff.hire\",\"agent.staff.terminate\"]' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
            [],
        )
        .unwrap();
    let room = store.open_human_agent_direct("alice", "manager").unwrap();
    let binding = store.load_execution_binding("alice", &room.id).unwrap();
    store
        .record_direct_delivery(
            "alice",
            &binding,
            "delivery-ui",
            "message-ui",
            "turn-ui",
            "run-ui",
            "请评估是否需要新成员",
            10,
        )
        .unwrap();
    assert!(store
        .communication_rooms_requiring_scheduler("alice")
        .unwrap()
        .is_empty());
    store
        .create_agent_creation_proposal(
            "alice",
            "run-ui",
            "proposal-direct",
            "候选 Agent",
            "工程师",
            "协助交付",
            "完成受托工作",
            None,
            "software_engineer",
            "存在长期职责缺口",
            11,
        )
        .unwrap();
    let proposal = store
        .list_agent_staffing_proposals("alice", &room.id)
        .unwrap()
        .remove(0);
    store
        .resolve_agent_staffing_proposal(
            "alice",
            &room.id,
            &proposal.id,
            "agentCreation",
            false,
            12,
        )
        .unwrap();
    assert_eq!(
        store
            .communication_rooms_requiring_scheduler("alice")
            .unwrap(),
        vec![room.id.clone()]
    );
    let pending = store
        .next_agent_communication_delivery("alice", &room.id)
        .unwrap()
        .unwrap();
    assert_eq!(pending.trigger_kind, "mention");
    assert!(pending.content.contains("Human 已拒绝"));
}

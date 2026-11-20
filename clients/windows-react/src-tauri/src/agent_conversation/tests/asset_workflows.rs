use super::*;

#[test]
fn manager_asset_run_resolves_and_loads_every_active_member_team() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "manager", "active");
    Connection::open(store.database())
        .unwrap()
        .execute(
            "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
            [],
        )
        .unwrap();
    let first = store
        .create_project_team("alice", "project-one", "第一团队", "维护第一组知识")
        .unwrap();
    let second = store
        .create_project_team("alice", "project-two", "第二团队", "维护第二组知识")
        .unwrap();
    for team_id in [&first.id, &second.id] {
        store
            .invite_agent_to_project_team("alice", team_id, "manager")
            .unwrap();
        store
            .set_project_team_manager("alice", team_id, "manager")
            .unwrap();
    }
    let queued = store
        .queue_project_team_message(
            "alice",
            &first.id,
            "跨团队维护共享资产",
            &["manager".into()],
        )
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &first.id)
        .unwrap()
        .unwrap();
    assert_eq!(delivery.message_id, queued.message_id);
    store
        .claim_project_team_delivery(
            "alice",
            &delivery,
            "turn-assets-multi-team",
            "run-assets-multi-team",
            10,
        )
        .unwrap();
    let first_asset = store
        .save_project_team_asset_as_manager(
            "alice",
            &first.id,
            "manager",
            None,
            "overview",
            "第一团队概览",
            "第一团队事实。",
            None,
            11,
        )
        .unwrap();
    let second_asset = store
        .save_project_team_asset_as_manager(
            "alice",
            &second.id,
            "manager",
            None,
            "overview",
            "第二团队概览",
            "第二团队事实。",
            None,
            12,
        )
        .unwrap();

    let (binding, records) = store
        .load_agent_asset_tool_records("alice", "run-assets-multi-team")
        .unwrap();
    assert_eq!(binding.team_id, first.id);
    assert!(binding.is_project_manager);
    assert_eq!(records.len(), 2);
    assert!(records
        .iter()
        .any(|record| record.team_id == first.id && record.asset_id == first_asset.id));
    assert!(records
        .iter()
        .any(|record| record.team_id == second.id && record.asset_id == second_asset.id));

    let second_reference =
        team_todo::todo_planning_reference("alice", "run-assets-multi-team", "team", &second.id);
    assert_eq!(
            store
                .resolve_agent_asset_team_reference(
                    "alice",
                    "run-assets-multi-team",
                    &second_reference,
                )
                .unwrap(),
            second.id
        );
    assert!(store
        .resolve_agent_asset_team_reference("alice", "run-assets-multi-team", "team_ref_forged",)
        .is_err());
}

#[test]
fn team_asset_suggestions_require_durable_manager_review_before_cycle_completion() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "manager", "active");
    insert_agent(&store, "alice", "member", "active");
    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
            [],
        )
        .unwrap();
    drop(connection);
    let team = store
        .create_project_team("alice", "project-review", "审核团队", "维护共享知识")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "manager")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "member")
        .unwrap();
    store
        .set_project_team_manager("alice", &team.id, "manager")
        .unwrap();

    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    connection
        .execute(
            r#"
                INSERT INTO local_agent_todos (
                    owner_user_id, id, agent_id, team_room_id, request_key,
                    title, detail, priority, sort_order, status, blocked_reason, result,
                    created_at_unix_ms, updated_at_unix_ms,
                    execution_plan_json, execution_contract_json
                ) VALUES ('alice', 'todo-review', 'member', ?1, 'request-review',
                          '完成架构验证', '', 80, 0, 'completed', '', '已完成',
                          100, 100, '{}', '{}')
                "#,
            params![team.id],
        )
        .unwrap();
    let architecture = AgentTeamAssetUpdateSuggestion {
        category: "architecture".into(),
        title: "事件驱动架构".into(),
        markdown: "采用事件驱动并保持幂等。".into(),
        rationale: "集成测试已经验证。".into(),
    };
    let convention = AgentTeamAssetUpdateSuggestion {
        category: "conventions".into(),
        title: "幂等约定".into(),
        markdown: "所有消费端必须支持重复投递。".into(),
        rationale: "故障恢复演练已经验证。".into(),
    };
    assert!(team_todo::insert_todo_progress(
        &connection,
        "alice",
        "todo-review",
        "run-invalid-duplicate",
        "completed",
        "completed",
        "不得保存重复建议。",
        &[architecture.clone(), architecture.clone()],
        101,
    )
    .is_err());
    assert!(team_todo::insert_todo_progress(
        &connection,
        "alice",
        "todo-review",
        "run-invalid-blocked",
        "blocked",
        "blocked",
        "阻塞事件不得携带建议。",
        std::slice::from_ref(&architecture),
        101,
    )
    .is_err());
    let oversized = AgentTeamAssetUpdateSuggestion {
        category: "reference".into(),
        title: "超大正文".into(),
        markdown: "架".repeat(100_000),
        rationale: "不得超过总字节门禁。".into(),
    };
    assert!(team_todo::insert_todo_progress(
        &connection,
        "alice",
        "todo-review",
        "run-invalid-bytes",
        "completed",
        "completed",
        "不得保存超大建议。",
        &[oversized],
        101,
    )
    .is_err());
    let too_many = vec![architecture.clone(); 9];
    assert!(team_todo::insert_todo_progress(
        &connection,
        "alice",
        "todo-review",
        "run-invalid-count",
        "completed",
        "completed",
        "不得保存过多建议。",
        &too_many,
        101,
    )
    .is_err());
    let invalid_event_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM local_agent_todo_events \
                 WHERE owner_user_id = 'alice' AND todo_id = 'todo-review'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(invalid_event_count, 0);
    team_todo::insert_todo_progress(
        &connection,
        "alice",
        "todo-review",
        "run-executor-review",
        "completed",
        "completed",
        "已完成并核验。",
        &[architecture, convention],
        102,
    )
    .unwrap();
    drop(connection);

    let pending = store
        .list_team_asset_reviews("alice", Some(&team.id), Some("manager"), "pending", 500)
        .unwrap();
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().all(|review| review.todo_id == "todo-review"));
    assert!(store
        .list_team_asset_reviews("alice", Some(&team.id), Some("member"), "pending", 500)
        .unwrap()
        .is_empty());
    assert!(store
        .list_team_asset_reviews("bob", Some(&team.id), None, "pending", 500)
        .unwrap()
        .is_empty());

    let queued = store
        .queue_project_team_message("alice", &team.id, "请审核共享资产建议", &["manager".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    assert_eq!(delivery.message_id, queued.message_id);
    assert_eq!(delivery.binding.agent.agent_id, "manager");
    store
        .claim_project_team_delivery(
            "alice",
            &delivery,
            "turn-review-manager",
            "run-review-manager",
            103,
        )
        .unwrap();
    let completion_error = store
        .validate_agent_communication_cycle_completion("alice", "run-review-manager", false)
        .unwrap_err();
    assert!(completion_error.contains("team_asset_reviews_pending"));
    assert!(completion_error.contains("team_asset_review_list"));

    let asset = store
        .save_project_team_asset_as_manager(
            "alice",
            &team.id,
            "manager",
            None,
            "architecture",
            "事件驱动架构",
            "采用事件驱动并保持幂等。",
            None,
            104,
        )
        .unwrap();
    let applied_review = pending
        .iter()
        .find(|review| review.suggestion.category == "architecture")
        .unwrap();
    assert!(store
        .resolve_team_asset_review(
            "alice",
            &applied_review.id,
            "member",
            "applied",
            Some((&asset.id, asset.revision)),
            "",
            105,
        )
        .is_err());
    let applied = store
        .resolve_team_asset_review(
            "alice",
            &applied_review.id,
            "manager",
            "applied",
            Some((&asset.id, asset.revision)),
            "",
            105,
        )
        .unwrap();
    assert_eq!(applied.status, "applied");
    assert_eq!(
        applied.resolved_asset_id.as_deref(),
        Some(asset.id.as_str())
    );
    assert!(store
        .resolve_team_asset_review(
            "alice",
            &applied_review.id,
            "manager",
            "applied",
            Some((&asset.id, asset.revision)),
            "",
            106,
        )
        .is_err());

    let rejected_review = pending
        .iter()
        .find(|review| review.suggestion.category == "conventions")
        .unwrap();
    assert!(store
        .resolve_team_asset_review(
            "alice",
            &rejected_review.id,
            "manager",
            "rejected",
            None,
            "",
            106,
        )
        .is_err());
    let rejected = store
        .resolve_team_asset_review(
            "alice",
            &rejected_review.id,
            "manager",
            "rejected",
            None,
            "该约定尚未覆盖所有消费者。",
            106,
        )
        .unwrap();
    assert_eq!(rejected.status, "rejected");
    assert!(store
        .list_team_asset_reviews("alice", Some(&team.id), Some("manager"), "pending", 500)
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .validate_agent_communication_cycle_completion("alice", "run-review-manager", false,)
            .unwrap()
            .1["status"],
        "completed"
    );
}

use super::*;

#[test]
fn project_team_messages_route_to_mentions_or_the_default_agent() {
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

    let default_routed = store
        .queue_project_team_message("alice", &created.id, "请开始处理", &[])
        .unwrap();
    assert_eq!(default_routed.deliveries.len(), 1);
    assert_eq!(default_routed.deliveries[0].target_agent_id, "agent-1");
    assert_eq!(default_routed.deliveries[0].trigger_kind, "default_agent");

    let mentioned = store
        .queue_project_team_message(
            "alice",
            &created.id,
            "请分别评估",
            &["agent-2".into(), "agent-1".into()],
        )
        .unwrap();
    assert_eq!(mentioned.deliveries.len(), 2);
    assert_eq!(mentioned.deliveries[0].target_agent_id, "agent-2");
    assert!(mentioned
        .deliveries
        .iter()
        .all(|delivery| delivery.trigger_kind == "mention"));
    let connection = Connection::open(store.database()).unwrap();
    let mention_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_message_mentions \
                 WHERE owner_user_id = 'alice' AND message_id = ?1",
            params![mentioned.message_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mention_count, 2);
    drop(connection);

    let snapshot = store
        .load_project_team_conversation("alice", &created.id, None, 50)
        .unwrap();
    assert_eq!(snapshot.team_id, created.id);
    assert_eq!(snapshot.messages.len(), 2);
    assert_eq!(
        snapshot.messages[1].mentioned_agent_ids,
        vec!["agent-2", "agent-1"]
    );
    assert_eq!(snapshot.deliveries.len(), 3);

    assert!(store
        .queue_project_team_message("alice", &created.id, "错误 @", &["missing".into()])
        .is_err());
    assert!(store
        .queue_project_team_message("bob", &created.id, "越权消息", &["agent-1".into()],)
        .is_err());
}

#[test]
fn project_team_conversation_pages_older_messages_without_overlap() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let team = store
        .create_project_team("alice", "project-1", "分页团队", "")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "agent-1")
        .unwrap();
    for content in ["第一条", "第二条", "第三条"] {
        store
            .queue_project_team_message("alice", &team.id, content, &[])
            .unwrap();
    }

    let recent = store
        .load_project_team_conversation("alice", &team.id, None, 2)
        .unwrap();
    assert_eq!(recent.messages.len(), 2);
    assert!(recent.has_older);
    let older = store
        .load_project_team_conversation(
            "alice",
            &team.id,
            recent.messages.first().map(|message| message.id.as_str()),
            2,
        )
        .unwrap();
    assert_eq!(older.messages.len(), 1);
    assert!(!older.has_older);
    assert!(recent
        .messages
        .iter()
        .all(|message| older.messages.iter().all(|item| item.id != message.id)));
}

#[test]
fn project_team_delivery_claims_the_exact_scoped_agent_binding() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let created = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &created.id, "agent-1")
        .unwrap();
    store
        .update_project_team_member(
            "alice",
            &created.id,
            "agent-1",
            "前端工程师",
            "负责 Windows UI",
        )
        .unwrap();
    let queued = store
        .queue_project_team_message("alice", &created.id, "实现团队页面", &[])
        .unwrap();
    let pending = store
        .next_project_team_delivery("alice", &created.id)
        .unwrap()
        .unwrap();
    assert_eq!(pending.delivery_id, queued.deliveries[0].delivery_id);
    assert_eq!(pending.binding.project_id, "project-1");
    assert_eq!(pending.binding.team_name, "Alpha 团队");
    assert_eq!(pending.binding.team_role, "前端工程师");
    assert_eq!(pending.binding.team_responsibility, "负责 Windows UI");
    assert!(!pending.binding.is_project_manager);

    store
        .claim_project_team_delivery("alice", &pending, "turn-1", "run-1", 100)
        .unwrap();
    let active = store
        .active_project_team_run_bindings("alice", &created.id)
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].delivery_id, pending.delivery_id);
    assert_eq!(
        active[0].conversation_id,
        format!("agent-team-v2-{}-agent-1", created.id)
    );
    assert_eq!(active[0].turn_id, "turn-1");
    assert_eq!(active[0].run_id, "run-1");
    assert!(store
        .next_project_team_delivery("alice", &created.id)
        .unwrap()
        .is_none());
    assert!(store
        .claim_project_team_delivery("alice", &pending, "turn-2", "run-2", 101)
        .is_err());
    store
        .fail_project_team_delivery("alice", &pending.delivery_id, "host unavailable")
        .unwrap();
    let connection = Connection::open(store.database()).unwrap();
    let state: (String, String, String) = connection
        .query_row(
            "SELECT delivery.status, run.status, delivery.last_error \
                 FROM project_agent_deliveries delivery JOIN windows_agent_delivery_runs run \
                   ON run.owner_user_id = delivery.owner_user_id \
                  AND run.delivery_id = delivery.id \
                 WHERE delivery.owner_user_id = 'alice' AND delivery.id = ?1",
            params![pending.delivery_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        state,
        ("failed".into(), "failed".into(), "host unavailable".into())
    );
    drop(connection);
    let history = store.load_project_team_runs("alice", &created.id).unwrap();
    assert_eq!(history.team_id, created.id);
    assert_eq!(history.message_count, 1);
    assert_eq!(history.failed_delivery_count, 1);
    assert_eq!(history.pending_delivery_count, 0);
    assert_eq!(history.running_delivery_count, 0);
    assert_eq!(history.runs.len(), 1);
    assert_eq!(history.runs[0].delivery_id, pending.delivery_id);
    assert_eq!(history.runs[0].agent_id, "agent-1");
    assert_eq!(history.runs[0].lane, "manager");
    assert_eq!(history.runs[0].trigger_kind, "default_agent");
    assert_eq!(history.runs[0].delivery_status, "failed");
    assert_eq!(history.runs[0].run_status, "failed");
    assert_eq!(
        history.runs[0].last_error.as_deref(),
        Some("host unavailable")
    );
    assert!(store.load_project_team_runs("bob", &created.id).is_err());
}

#[test]
fn ready_todo_uses_an_independent_executor_lane_and_blocks_on_host_failure() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let team = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "agent-1")
        .unwrap();
    let manager_message = store
        .queue_project_team_message("alice", &team.id, "经理通道消息", &[])
        .unwrap();
    let manager = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        manager.delivery_id,
        manager_message.deliveries[0].delivery_id
    );
    assert!(manager.execution_plan.is_none());
    store
        .claim_project_team_delivery("alice", &manager, "turn-manager", "run-manager", 100)
        .unwrap();
    let shared_asset = store
        .save_project_team_asset(
            "alice",
            &team.id,
            None,
            "overview",
            "项目说明",
            "启动时版本",
            None,
            100,
        )
        .unwrap();

    let first = store
        .create_project_team_todo(
            "alice",
            &team.id,
            "agent-1",
            "实现任务执行",
            "连接 executor lane",
            90,
            &[],
            None,
            101,
        )
        .unwrap();
    let second = store
        .create_project_team_todo(
            "alice",
            &team.id,
            "agent-1",
            "后续任务",
            "等待第一个任务结束",
            80,
            &[],
            None,
            102,
        )
        .unwrap();
    let project_write_plan = serde_json::json!({
        "requiresExecution": true,
        "builtinCapabilities": ["project_read", "project_write"],
        "plugins": [],
        "selectionRevision": "test-project-write-v1",
        "selectedAtUnixMs": 102
    })
    .to_string();
    Connection::open(store.database())
        .unwrap()
        .execute(
            "UPDATE local_agent_todos SET execution_plan_json = ?1 \
                 WHERE owner_user_id = 'alice' AND id = ?2",
            params![project_write_plan, second.id],
        )
        .unwrap();
    assert_eq!(
        store
            .enqueue_ready_project_team_todos("alice", &team.id, 103)
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .enqueue_ready_project_team_todos("alice", &team.id, 104)
            .unwrap(),
        0
    );
    let executor = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    assert_eq!(executor.trigger_kind, "todo");
    assert_eq!(executor.todo_id.as_deref(), Some(first.id.as_str()));
    assert_eq!(
        executor
            .execution_plan
            .as_ref()
            .map(|plan| plan.builtin_capabilities.as_slice()),
        Some(["project_read".to_owned()].as_slice())
    );
    assert!(executor.content.contains("独立 Todo"));
    assert!(executor.content.contains("实现任务执行"));
    store
        .claim_project_team_delivery("alice", &executor, "turn-executor", "run-executor", 105)
        .unwrap();
    assert!(store
        .team_chat_member_snapshot("alice", "run-executor")
        .is_err());
    let (executor_binding, executor_assets) = store
        .load_agent_asset_tool_records("alice", "run-executor")
        .unwrap();
    assert_eq!(executor_binding.todo_id.as_deref(), Some(first.id.as_str()));
    assert_eq!(executor_assets.len(), 1);
    assert_eq!(executor_assets[0].markdown, "启动时版本");
    store
        .save_project_team_asset(
            "alice",
            &team.id,
            Some(&shared_asset.id),
            "overview",
            "项目说明",
            "任务启动后的版本",
            Some(1),
            106,
        )
        .unwrap();
    let (_, frozen_assets) = store
        .load_agent_asset_tool_records("alice", "run-executor")
        .unwrap();
    assert_eq!(frozen_assets[0].revision, 1);
    assert_eq!(frozen_assets[0].markdown, "启动时版本");
    let (manager_binding, manager_assets) = store
        .load_agent_asset_tool_records("alice", "run-manager")
        .unwrap();
    assert!(manager_binding.todo_id.is_none());
    assert_eq!(manager_assets[0].revision, 2);
    let (planning_binding, planning_snapshot) = store
        .todo_planning_tool_snapshot("alice", "run-manager", true)
        .unwrap();
    assert_eq!(planning_binding, manager_binding);
    assert_eq!(planning_snapshot["team"]["name"], "Alpha 团队");
    assert_eq!(planning_snapshot["todos"].as_array().unwrap().len(), 2);
    assert_eq!(
        planning_snapshot["builtin_capabilities"],
        serde_json::json!([
            "project_read",
            "project_write",
            "terminal",
            "requirement_survey_read",
            "requirement_survey_write"
        ])
    );
    let todo_reference = planning_snapshot["todos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|todo| todo["title"] == "实现任务执行")
        .unwrap()["todo_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!planning_snapshot.to_string().contains(&first.id));
    let (_, progress) = store
        .todo_planning_progress("alice", "run-manager", &todo_reference, 100)
        .unwrap();
    assert_eq!(progress["title"], "实现任务执行");
    assert_eq!(progress["progress"].as_array().unwrap().len(), 1);
    assert_eq!(progress["progress"][0]["kind"], "started");
    assert_eq!(progress["progress"][0]["run_id"], "run-executor");
    assert!(store
        .todo_planning_progress("alice", "run-manager", "todo_ref_stale", 100)
        .is_err());

    let connection = Connection::open(store.database()).unwrap();
    let running_lanes: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_deliveries WHERE owner_user_id = 'alice' \
                 AND target_agent_id = 'agent-1' AND status = 'running'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(running_lanes, 2);
    drop(connection);

    store
        .fail_project_team_delivery("alice", &executor.delivery_id, "host unavailable")
        .unwrap();
    let failed_todo = store
        .list_project_team_todos("alice", &team.id, true)
        .unwrap()
        .into_iter()
        .find(|todo| todo.id == first.id)
        .unwrap();
    assert_eq!(failed_todo.status, "blocked");
    assert_eq!(failed_todo.blocked_reason, "host unavailable");

    assert_eq!(
        store
            .enqueue_ready_project_team_todos("alice", &team.id, 106)
            .unwrap(),
        1
    );
    let second_executor = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    assert_eq!(second_executor.todo_id.as_deref(), Some(second.id.as_str()));
    store
        .claim_project_team_delivery(
            "alice",
            &second_executor,
            "turn-executor-2",
            "run-executor-2",
            107,
        )
        .unwrap();
    let (_, execution_context) = store
        .todo_execution_tool_context("alice", "run-executor-2")
        .unwrap();
    assert_eq!(execution_context["title"], "后续任务");
    let (_, progress) = store
        .append_todo_execution_progress(
            "alice",
            "run-executor-2",
            "implementation",
            "已完成事务写入，准备核对。",
            108,
        )
        .unwrap();
    assert_eq!(progress["kind"], "progress");
    let visible_progress = store
        .list_project_team_todo_progress("alice", &team.id, &second.id, 100)
        .unwrap();
    assert_eq!(visible_progress.len(), 2);
    assert_eq!(visible_progress[0]["kind"], "started");
    assert_eq!(visible_progress[1]["detail"], "已完成事务写入，准备核对。");
    assert_eq!(visible_progress[0]["runId"], "run-executor-2");
    assert_eq!(visible_progress[0]["createdAtUnixMs"], 107);
    assert!(visible_progress[0].get("run_id").is_none());
    assert!(visible_progress[0].get("created_at_unix_ms").is_none());
    assert!(store
        .list_project_team_todo_progress("bob", &team.id, &second.id, 100)
        .is_err());
    assert!(store
        .finish_todo_execution(
            "alice",
            "run-executor-2",
            true,
            "已完成并核验。",
            &[],
            false,
            109,
        )
        .is_err());
    let (_, completion) = store
        .finish_todo_execution(
            "alice",
            "run-executor-2",
            true,
            "已完成并核验。",
            &[],
            true,
            109,
        )
        .unwrap();
    assert_eq!(completion["todo_status"], "completed");
    store
        .reconcile_project_team_runs(
            "alice",
            &team.id,
            &[team_todo::TeamRunReconciliation {
                delivery_id: second_executor.delivery_id,
                turn_id: "turn-executor-2".into(),
                run_id: "run-executor-2".into(),
                status: LocalConversationTurnStatus::Succeeded,
                created_at_unix_ms: 107,
                updated_at_unix_ms: 110,
                project_write_evidence_verified: true,
            }],
        )
        .unwrap();
    let completed = store
        .list_project_team_todos("alice", &team.id, true)
        .unwrap()
        .into_iter()
        .find(|todo| todo.id == second.id)
        .unwrap();
    assert_eq!(completed.status, "completed");
}

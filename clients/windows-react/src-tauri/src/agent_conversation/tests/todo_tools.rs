use super::*;

#[test]
fn manager_todo_planning_tools_create_update_and_reorder_transactionally() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "manager", "active");
    insert_agent(&store, "alice", "worker", "active");
    Connection::open(store.database())
        .unwrap()
        .execute(
            "UPDATE local_agent_profiles SET profession_key = 'project_manager' \
                 WHERE owner_user_id = 'alice' AND id = 'manager'",
            [],
        )
        .unwrap();
    let team = store
        .create_project_team("alice", "project-1", "计划团队", "完成可靠交付")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "manager")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "worker")
        .unwrap();
    store
        .set_project_team_manager("alice", &team.id, "manager")
        .unwrap();
    let message = store
        .queue_project_team_message("alice", &team.id, "请拆分并执行这项工作", &[])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    assert_eq!(delivery.delivery_id, message.deliveries[0].delivery_id);
    store
        .claim_project_team_delivery("alice", &delivery, "turn-manager", "run-manager", 10)
        .unwrap();
    let (member_binding, members) = store
        .team_chat_member_snapshot("alice", "run-manager")
        .unwrap();
    assert_eq!(member_binding.room_id, team.id);
    assert_eq!(members.as_array().unwrap().len(), 2);
    assert!(members
        .as_array()
        .unwrap()
        .iter()
        .any(|member| member["name"] == "测试 Agent" && member["is_project_manager"] == true));
    assert!(!members.to_string().contains("\"manager\""));
    assert!(!members.to_string().contains("\"worker\""));
    let (_, options) = store
        .todo_planning_tool_snapshot("alice", "run-manager", true)
        .unwrap();
    let team_reference = options["team"]["team_ref"].as_str().unwrap().to_owned();
    let worker_reference = options["team"]["assignees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|assignee| {
            assignee["name"] == "测试 Agent" && !assignee["is_current_agent"].as_bool().unwrap()
        })
        .unwrap()["assignee_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let contract = team_todo::AgentTodoExecutionContract {
        objective: "实现核心功能".into(),
        scope: "只修改项目内代码".into(),
        expected_outputs: vec!["可运行实现".into()],
        acceptance_criteria: vec!["测试通过".into()],
        constraints: vec!["不得伪造完成".into()],
    };
    let (binding, first) = store
        .create_todo_from_planning_tool(
            "alice",
            "run-manager",
            team_todo::TodoPlanningCreateInput {
                request_key: "invocation-create-1".into(),
                team_reference: team_reference.clone(),
                assignee_reference: worker_reference.clone(),
                title: "实现核心".into(),
                detail: "按验收条件完成".into(),
                priority: 80,
                dependency_references: vec![],
                execution_contract: contract.clone(),
                requires_execution: true,
                builtin_capabilities: vec!["project_write".into()],
                plugins: vec![team_todo::AgentTodoPluginSelection {
                    installation_id: "installation-document".into(),
                    plugin_id: "document-plugin".into(),
                    component_key: "document-mcp".into(),
                    component_revision: "revision-1".into(),
                    release_id: "release-1".into(),
                    release_digest: format!("sha256:{}", "a".repeat(64)),
                    manifest_version: "1.2.3".into(),
                    display_name: "Document Tools".into(),
                    reason: "生成验收文档".into(),
                }],
            },
            11,
        )
        .unwrap();
    assert!(binding.is_project_manager);
    assert_eq!(
        first["builtin_capabilities"],
        json!(["project_read", "project_write"])
    );
    assert_eq!(first["plugins"], json!(["Document Tools"]));
    let first_reference = first["todo_ref"].as_str().unwrap().to_owned();
    let (_, second) = store
        .create_todo_from_planning_tool(
            "alice",
            "run-manager",
            team_todo::TodoPlanningCreateInput {
                request_key: "invocation-create-2".into(),
                team_reference,
                assignee_reference: worker_reference,
                title: "执行验证".into(),
                detail: "验证第一项交付".into(),
                priority: 70,
                dependency_references: vec![first_reference.clone()],
                execution_contract: team_todo::AgentTodoExecutionContract {
                    objective: "验证交付".into(),
                    scope: "运行真实测试".into(),
                    expected_outputs: vec!["测试记录".into()],
                    acceptance_criteria: vec!["结果可复现".into()],
                    constraints: vec![],
                },
                requires_execution: true,
                builtin_capabilities: vec!["terminal".into()],
                plugins: vec![],
            },
            12,
        )
        .unwrap();
    assert_eq!(second["dependencies"][0]["todo_ref"], first_reference);
    let second_reference = second["todo_ref"].as_str().unwrap().to_owned();
    let (_, updated) = store
        .update_todo_from_planning_tool(
            "alice",
            "run-manager",
            team_todo::TodoPlanningUpdateInput {
                todo_reference: first_reference.clone(),
                title: Some("实现核心并自测".into()),
                detail: None,
                priority: Some(95),
                status: None,
                dependency_references: None,
                objective: None,
                scope: None,
                expected_outputs: None,
                acceptance_criteria: None,
                constraints: None,
            },
            13,
        )
        .unwrap();
    assert_eq!(updated["title"], "实现核心并自测");
    let (_, reordered) = store
        .reorder_todos_from_planning_tool(
            "alice",
            "run-manager",
            &[second_reference, first_reference],
            14,
        )
        .unwrap();
    assert_eq!(reordered["todos"][0]["title"], "执行验证");
    let source_count: i64 = Connection::open(store.database())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM local_agent_todo_sources source \
                 JOIN project_agent_messages message \
                   ON message.owner_user_id = source.owner_user_id \
                  AND message.id = source.message_id \
                 WHERE source.owner_user_id = 'alice' AND message.id = ?1",
            params![message.message_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_count, 3);
    assert!(store
        .team_todo_scheduler_has_work("alice", &team.id)
        .unwrap());
    store
        .reconcile_project_team_runs(
            "alice",
            &team.id,
            &[team_todo::TeamRunReconciliation {
                delivery_id: delivery.delivery_id,
                turn_id: "turn-manager".into(),
                run_id: "run-manager".into(),
                status: LocalConversationTurnStatus::Succeeded,
                created_at_unix_ms: 10,
                updated_at_unix_ms: 15,
                project_write_evidence_verified: false,
            }],
        )
        .unwrap();
    assert!(!store
        .team_todo_scheduler_has_work("alice", &team.id)
        .unwrap());
    assert_eq!(
        store
            .enqueue_ready_project_team_todos("alice", &team.id, 16)
            .unwrap(),
        1
    );
    let executor = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    let plan = executor.execution_plan.unwrap();
    assert_eq!(plan.plugins.len(), 1);
    assert_eq!(plan.plugins[0].component_key, "document-mcp");
    assert_eq!(plan.plugins[0].release_id, "release-1");
    assert!(store
        .team_todo_scheduler_has_work("alice", &team.id)
        .unwrap());
    assert_eq!(
        store.teams_requiring_todo_scheduler("alice").unwrap(),
        vec![team.id]
    );
}

#[test]
fn team_agent_messages_use_run_scoped_mentions_and_idempotent_receipts() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "sender", "active");
    insert_agent(&store, "alice", "target", "active");
    let team = store
        .create_project_team("alice", "project-1", "通讯团队", "交付消息")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "sender")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "target")
        .unwrap();
    let trigger = store
        .queue_project_team_message("alice", &team.id, "请协调目标成员", &["sender".into()])
        .unwrap();
    let delivery = store
        .next_project_team_delivery("alice", &team.id)
        .unwrap()
        .unwrap();
    store
        .claim_project_team_delivery("alice", &delivery, "turn-chat", "run-chat", 10)
        .unwrap();
    let (_, members) = store
        .team_chat_member_snapshot("alice", "run-chat")
        .unwrap();
    let target_reference = members
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["is_current_agent"] == false)
        .unwrap()["agent_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, first) = store
        .send_team_chat_message(
            "alice",
            "run-chat",
            "invocation-chat-1",
            "请处理接口验证。",
            None,
            None,
            None,
            &[],
            std::slice::from_ref(&target_reference),
            11,
        )
        .unwrap();
    assert_eq!(first["completed"], false);
    assert_eq!(first["spawned_delivery_count"], 1);
    assert!(!first.to_string().contains("target"));
    let (_, replayed) = store
        .send_team_chat_message(
            "alice",
            "run-chat",
            "invocation-chat-1",
            "请处理接口验证。",
            None,
            None,
            None,
            &[],
            std::slice::from_ref(&target_reference),
            12,
        )
        .unwrap();
    assert_eq!(replayed, first);
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-chat",
            "invocation-chat-1",
            "复用调用 ID 的不同消息。",
            None,
            None,
            None,
            &[],
            std::slice::from_ref(&target_reference),
            13,
        )
        .is_err());
    assert!(store
        .send_team_chat_message(
            "alice",
            "run-chat",
            "invocation-chat-stale-ref",
            "错误引用。",
            None,
            None,
            None,
            &[],
            &["agent_ref_stale".into()],
            13,
        )
        .is_err());
    let connection = Connection::open(store.database()).unwrap();
    let message: (String, String, String, String, i64) = connection
        .query_row(
            "SELECT sender_id, reply_to_message_id, source_run_id, causation_id, hop_count \
                 FROM project_agent_messages WHERE owner_user_id = 'alice' \
                   AND causation_id = 'invocation-chat-1'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        message,
        (
            "sender".into(),
            trigger.message_id.clone(),
            "run-chat".into(),
            "invocation-chat-1".into(),
            1,
        )
    );
    let spawned: (i64, String, String) = connection
        .query_row(
            "SELECT count(*), min(target_agent_id), min(trigger_kind) \
                 FROM project_agent_deliveries WHERE owner_user_id = 'alice' \
                   AND trigger_kind = 'agent_mention'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(spawned, (1, "target".into(), "agent_mention".into()));
    connection
        .execute(
            "UPDATE project_agent_messages SET hop_count = 4 \
                 WHERE owner_user_id = 'alice' AND id = ?1",
            params![trigger.message_id],
        )
        .unwrap();
    drop(connection);
    let (_, unread) = store
        .read_all_unread_team_chat("alice", "run-chat", "read-routing-limit", 20, 14)
        .unwrap();
    let trigger_reference = unread["conversations"][0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["content"] == "请协调目标成员")
        .unwrap()["message_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, stopped) = store
        .send_team_chat_message(
            "alice",
            "run-chat",
            "invocation-routing-limit",
            "这条回复只持久化，不再继续唤醒。",
            Some(&trigger_reference),
            None,
            None,
            &[],
            std::slice::from_ref(&target_reference),
            15,
        )
        .unwrap();
    assert_eq!(stopped["spawned_delivery_count"], 0);
    assert_eq!(
        stopped["routing_stop_reason"],
        "本轮 Agent 协作已达到最大唤醒深度。"
    );
    let stopped_delivery_count: i64 = Connection::open(store.database())
        .unwrap()
        .query_row(
            "SELECT count(*) FROM project_agent_deliveries delivery \
                 JOIN project_agent_messages message \
                   ON message.owner_user_id = delivery.owner_user_id \
                  AND message.id = delivery.message_id \
                 WHERE message.causation_id = 'invocation-routing-limit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stopped_delivery_count, 0);
}

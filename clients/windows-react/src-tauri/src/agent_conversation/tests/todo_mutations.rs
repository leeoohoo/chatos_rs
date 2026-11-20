use super::*;

#[test]
fn project_team_todos_are_transactional_account_scoped_and_mac_compatible() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let team = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "agent-1")
        .unwrap();
    let execution_plan = serde_json::json!({
        "requiresExecution": true,
        "builtinCapabilities": ["project_read", "project_write"],
        "plugins": [],
        "selectionRevision": "local-v1",
        "selectedAtUnixMs": 100
    })
    .to_string();
    let execution_contract = serde_json::json!({
        "objective": "实现任务看板",
        "scope": "Windows React 团队页",
        "expectedOutputs": ["可核验的任务列表"],
        "acceptanceCriteria": ["严格隔离团队与账号"],
        "constraints": ["参考 macOS 语义"]
    })
    .to_string();

    let mut connection = Connection::open(store.database()).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    {
        let transaction = connection.transaction().unwrap();
        transaction
            .execute(
                r#"
                    INSERT INTO local_agent_todos (
                        owner_user_id, id, agent_id, team_room_id, request_key,
                        title, detail, priority, sort_order, status, blocked_reason, result,
                        created_at_unix_ms, updated_at_unix_ms,
                        execution_plan_json, execution_contract_json
                    ) VALUES ('alice', 'rolled-back', 'agent-1', ?1, 'request-rollback',
                              '不应保留', '', 50, 0, 'pending', '', '', 100, 100, ?2, ?3)
                    "#,
                params![team.id, execution_plan, execution_contract],
            )
            .unwrap();
        assert!(transaction
            .execute(
                "INSERT INTO local_agent_todo_dependencies (owner_user_id, todo_id, \
                     prerequisite_todo_id, created_at_unix_ms) \
                     VALUES ('alice', 'rolled-back', 'rolled-back', 100)",
                [],
            )
            .is_err());
    }
    let rolled_back: i64 = connection
        .query_row(
            "SELECT count(*) FROM local_agent_todos WHERE id = 'rolled-back'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rolled_back, 0);

    let transaction = connection.transaction().unwrap();
    for (id, request_key, title, priority, sort_order, status) in [
        (
            "todo-prerequisite",
            "request-1",
            "先完成底座",
            80,
            0,
            "completed",
        ),
        ("todo-active", "request-2", "再实现看板", 70, 1, "pending"),
    ] {
        transaction
            .execute(
                r#"
                    INSERT INTO local_agent_todos (
                        owner_user_id, id, agent_id, team_room_id, request_key,
                        title, detail, priority, sort_order, status, blocked_reason, result,
                        created_at_unix_ms, updated_at_unix_ms,
                        execution_plan_json, execution_contract_json
                    ) VALUES ('alice', ?1, 'agent-1', ?2, ?3, ?4, '', ?5, ?6, ?7,
                              '', '', 100, 100, ?8, ?9)
                    "#,
                params![
                    id,
                    team.id,
                    request_key,
                    title,
                    priority,
                    sort_order,
                    status,
                    execution_plan,
                    execution_contract,
                ],
            )
            .unwrap();
    }
    transaction
        .execute(
            "INSERT INTO local_agent_todo_dependencies (owner_user_id, todo_id, \
                 prerequisite_todo_id, created_at_unix_ms) \
                 VALUES ('alice', 'todo-active', 'todo-prerequisite', 100)",
            [],
        )
        .unwrap();
    transaction.commit().unwrap();
    drop(connection);

    let active = store
        .list_project_team_todos("alice", &team.id, false)
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "todo-active");
    assert_eq!(active[0].agent_name, "测试 Agent");
    assert_eq!(active[0].dependency_ids, vec!["todo-prerequisite"]);
    assert_eq!(active[0].execution_contract.objective, "实现任务看板");
    assert_eq!(
        active[0].execution_plan.builtin_capabilities,
        vec!["project_read", "project_write"]
    );
    assert_eq!(
        store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .len(),
        2
    );
    assert!(store
        .list_project_team_todos("bob", &team.id, true)
        .is_err());
}

#[test]
fn project_team_todo_mutations_reject_stale_cycles_and_unsafe_running_cancel() {
    let (_directory, store) = store();
    insert_agent(&store, "alice", "agent-1", "active");
    let team = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();
    store
        .invite_agent_to_project_team("alice", &team.id, "agent-1")
        .unwrap();
    let foundation = store
        .create_project_team_todo(
            "alice",
            &team.id,
            "agent-1",
            "实现持久化底座",
            "先完成任务事实源",
            80,
            &[],
            None,
            100,
        )
        .unwrap();
    let board = store
        .create_project_team_todo(
            "alice",
            &team.id,
            "agent-1",
            "实现任务看板",
            "再连接 React",
            70,
            std::slice::from_ref(&foundation.id),
            None,
            101,
        )
        .unwrap();
    assert!(store
        .update_project_team_todo(
            "alice",
            &team.id,
            &foundation.id,
            foundation.updated_at_unix_ms,
            &foundation.title,
            "循环依赖不应保存",
            foundation.priority,
            std::slice::from_ref(&board.id),
            None,
            102,
        )
        .is_err());

    let revised = store
        .update_project_team_todo(
            "alice",
            &team.id,
            &board.id,
            board.updated_at_unix_ms,
            "实现 Apple 风格任务看板",
            "连接真实 Todo",
            75,
            std::slice::from_ref(&foundation.id),
            None,
            103,
        )
        .unwrap();
    assert_eq!(revised.title, "实现 Apple 风格任务看板");
    assert!(store
        .update_project_team_todo(
            "alice",
            &team.id,
            &board.id,
            board.updated_at_unix_ms,
            "过期更新",
            "不应保存",
            20,
            &[],
            None,
            104,
        )
        .is_err());

    let reordered = store
        .reorder_project_team_todos(
            "alice",
            &team.id,
            &[board.id.clone(), foundation.id.clone()],
            105,
        )
        .unwrap();
    assert_eq!(reordered[0].id, board.id);
    assert_eq!(reordered[0].priority, 100);

    let foundation_after_reorder = reordered
        .iter()
        .find(|todo| todo.id == foundation.id)
        .unwrap();
    let cancelled = store
        .cancel_project_team_todo(
            "alice",
            &team.id,
            &foundation.id,
            foundation_after_reorder.updated_at_unix_ms,
            106,
        )
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");

    let running = store
        .create_project_team_todo(
            "alice",
            &team.id,
            "agent-1",
            "执行中的任务",
            "不能伪装成已取消",
            60,
            &[],
            None,
            107,
        )
        .unwrap();
    let connection = Connection::open(store.database()).unwrap();
    connection
        .execute(
            "UPDATE local_agent_todos SET status = 'in_progress' \
                 WHERE owner_user_id = 'alice' AND id = ?1",
            params![running.id],
        )
        .unwrap();
    assert!(store
        .cancel_project_team_todo(
            "alice",
            &team.id,
            &running.id,
            running.updated_at_unix_ms,
            108,
        )
        .is_err());
    assert_eq!(
        store
            .list_project_team_todos("alice", &team.id, true)
            .unwrap()
            .into_iter()
            .find(|todo| todo.id == running.id)
            .unwrap()
            .status,
        "in_progress"
    );
}

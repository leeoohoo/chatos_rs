use super::*;

#[test]
fn project_dashboard_is_revisioned_and_only_the_active_manager_can_write() {
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
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
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

    let first = AgentProjectDashboardUpdate {
        phase: "实现".into(),
        health: "on_track".into(),
        summary: "核心链路正在实现。".into(),
        next_steps: vec!["完成回归".into()],
        milestones: vec![AgentProjectMilestone {
            id: "milestone-1".into(),
            title: "Windows 首版".into(),
            detail: "完成原生打包".into(),
            status: "in_progress".into(),
            progress_percent: 60,
            acceptance_criteria: vec!["安装包可运行".into()],
            linked_todo_ids: vec![],
            target_at_unix_ms: None,
        }],
        issues: vec![AgentProjectIssue {
            id: "issue-1".into(),
            title: "确认发布范围".into(),
            detail: "需要产品方向决定".into(),
            requested_action: "请确认是否包含自动更新".into(),
            severity: "warning".into(),
            owner: "human".into(),
            related_todo_id: None,
        }],
    };
    let created = store
        .save_project_dashboard_as_manager("alice", &team.id, "manager", None, first.clone(), 100)
        .unwrap();
    assert_eq!(created.revision, 1);
    assert_eq!(created.phase, "实现");
    assert_eq!(
        store
            .load_project_dashboard_snapshot("alice", &team.id)
            .unwrap()
            .dashboard,
        Some(created.clone())
    );
    assert!(
        store
            .save_project_dashboard_as_manager(
                "alice",
                &team.id,
                "member",
                Some(1),
                first.clone(),
                150,
            )
            .is_err()
    );

    let second = AgentProjectDashboardUpdate {
        phase: "联调".into(),
        health: "at_risk".into(),
        summary: "等待产品确认。".into(),
        ..first.clone()
    };
    let updated = store
        .save_project_dashboard_as_manager(
            "alice",
            &team.id,
            "manager",
            Some(1),
            second.clone(),
            200,
        )
        .unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.phase, "联调");
    assert!(store
        .save_project_dashboard_as_manager("alice", &team.id, "manager", Some(1), second, 300,)
        .is_err());
}

#[test]
fn project_team_assets_keep_revision_history_and_reject_stale_writes() {
    let (_directory, store) = store();
    let team = store
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
        .unwrap();

    let created = store
        .save_project_team_asset(
            "alice",
            &team.id,
            None,
            "overview",
            "项目概览",
            "# Alpha\n\n第一版",
            None,
            100,
        )
        .unwrap();
    assert_eq!(created.revision, 1);
    assert_eq!(
        store.list_project_team_assets("alice", &team.id).unwrap(),
        vec![created.clone()]
    );

    let updated = store
        .save_project_team_asset(
            "alice",
            &team.id,
            Some(&created.id),
            "current_progress",
            "当前进度",
            "# Alpha\n\n第二版",
            Some(1),
            200,
        )
        .unwrap();
    assert_eq!(updated.revision, 2);
    assert!(store
        .save_project_team_asset(
            "alice",
            &team.id,
            Some(&created.id),
            "overview",
            "过期写入",
            "不会保存",
            Some(1),
            300,
        )
        .is_err());
    let revisions = store
        .list_project_team_asset_revisions("alice", &team.id, &created.id)
        .unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].revision, 2);
    assert_eq!(revisions[1].revision, 1);

    let archived = store
        .archive_project_team_asset("alice", &team.id, &created.id, 2, 300)
        .unwrap();
    assert_eq!(archived.status, "archived");
    assert_eq!(archived.revision, 3);
    assert!(store
        .list_project_team_assets("alice", &team.id)
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .list_project_team_asset_revisions("alice", &team.id, &created.id)
            .unwrap()
            .len(),
        3
    );
    assert!(store.list_project_team_assets("bob", &team.id).is_err());
}

#[test]
fn only_the_active_project_manager_agent_can_mutate_team_assets() {
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
        .create_project_team("alice", "project-1", "Alpha 团队", "交付 Alpha")
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
    assert!(store
        .teams_requiring_todo_scheduler("alice")
        .unwrap()
        .is_empty());

    assert!(store
        .save_project_team_asset_as_manager(
            "alice",
            &team.id,
            "member",
            None,
            "overview",
            "越权",
            "不能保存",
            None,
            100,
        )
        .is_err());
    let created = store
        .save_project_team_asset_as_manager(
            "alice",
            &team.id,
            "manager",
            None,
            "overview",
            "项目概览",
            "经理创建",
            None,
            100,
        )
        .unwrap();
    assert_eq!(created.created_by_agent_id.as_deref(), Some("manager"));
    let updated = store
        .save_project_team_asset_as_manager(
            "alice",
            &team.id,
            "manager",
            Some(&created.id),
            "current_progress",
            "当前进度",
            "经理更新",
            Some(1),
            200,
        )
        .unwrap();
    assert_eq!(updated.updated_by_agent_id.as_deref(), Some("manager"));
    let archived = store
        .archive_project_team_asset_as_manager("alice", &team.id, "manager", &created.id, 2, 300)
        .unwrap();
    assert_eq!(archived.updated_by_agent_id.as_deref(), Some("manager"));
    let revisions = store
        .list_project_team_asset_revisions("alice", &team.id, &created.id)
        .unwrap();
    assert!(revisions
        .iter()
        .all(|revision| revision.editor_agent_id.as_deref() == Some("manager")));
}

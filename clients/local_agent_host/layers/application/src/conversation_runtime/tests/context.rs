#[test]
fn agent_context_requires_a_matching_contact_resource() {
    let resource = LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Contact,
        resource_id: "agent-1".into(),
    };
    let normalized = normalized_agent_execution_context(
        &agent_context(),
        "agent-direct-v2-room-1",
        Some(&resource),
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(normalized["agent_id"], "agent-1");
    assert_eq!(normalized["profession_key"], "software_engineer");
    assert!(normalized_agent_execution_context(
        &agent_context(),
        "agent-direct-v2-room-1",
        None,
        None,
    )
    .is_err());
    assert!(normalized_agent_execution_context(
        &agent_context(),
        "agent-direct-v2-room-1",
        Some(&project_resource()),
        None,
    )
    .is_err());
}

#[test]
fn ordinary_conversation_identity_comes_from_the_client_and_matches_its_resource() {
    let jiguli = normalized_conversation_agent_identity(
        &json!({"agent_id": "jiguli", "agent_name": "叽咕狸"}),
        Some(&project_resource()),
    )
    .expect("Jiguli project identity");
    assert_eq!(jiguli, ("jiguli".to_string(), Some("叽咕狸".to_string())));

    let plugin_resource = LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Contact,
        resource_id: "plugin-installer".into(),
    };
    let plugin = normalized_conversation_agent_identity(
        &json!({"agent_id": "plugin-installer", "agent_name": "插件安装助手"}),
        Some(&plugin_resource),
    )
    .expect("plugin Agent identity");
    assert_eq!(plugin.0, "plugin-installer");
    assert!(normalized_conversation_agent_identity(
        &json!({"agent_id": "jiguli"}),
        Some(&plugin_resource),
    )
    .is_err());
    assert!(normalized_conversation_agent_identity(
        &json!({"agent_id": "another-agent"}),
        Some(&project_resource()),
    )
    .is_err());
    assert!(normalized_conversation_agent_identity(&json!({"agent_id": "jiguli"}), None,).is_err());
}

#[test]
fn agent_context_rejects_unknown_fields_and_bad_skill_hashes() {
    let resource = LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Contact,
        resource_id: "agent-1".into(),
    };
    let mut unknown = agent_context();
    unknown["agent_execution_context"]["secret_path"] = json!("/tmp/private");
    assert!(normalized_agent_execution_context(
        &unknown,
        "agent-direct-v2-room-1",
        Some(&resource),
        None,
    )
    .is_err());

    let mut bad_hash = agent_context();
    bad_hash["agent_execution_context"]["profession_skill_sha256"] = json!("ABC");
    assert!(normalized_agent_execution_context(
        &bad_hash,
        "agent-direct-v2-room-1",
        Some(&resource),
        None,
    )
    .is_err());
}

#[test]
fn team_agent_context_is_strictly_bound_to_its_agent_lane() {
    let resource = LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Contact,
        resource_id: "agent-1".into(),
    };
    let normalized = normalized_agent_execution_context(
        &team_agent_context(),
        "agent-team-v2-team-1-agent-1",
        Some(&resource),
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(normalized["schema_version"], 2);
    assert_eq!(normalized["team_role"], "前端工程师");
    assert_eq!(normalized["is_project_manager"], false);
    assert!(normalized_agent_execution_context(
        &team_agent_context(),
        "agent-team-v2-team-1-agent-2",
        Some(&resource),
        None,
    )
    .is_err());
    let mut wrong_source = team_agent_context();
    wrong_source["source"] = json!("native_agent_direct");
    assert!(normalized_agent_execution_context(
        &wrong_source,
        "agent-team-v2-team-1-agent-1",
        Some(&resource),
        None,
    )
    .is_err());
}

#[test]
fn executor_context_is_project_bound_and_capability_scoped() {
    let metadata = team_executor_context();
    let executor =
        normalized_team_executor_context(&metadata, "agent-team-executor-delivery-1", None)
            .unwrap()
            .unwrap();
    let normalized = normalized_agent_execution_context(
        &metadata,
        "agent-team-executor-delivery-1",
        None,
        Some(&executor),
    )
    .unwrap()
    .unwrap();
    assert_eq!(normalized["project_id"], "project-1");
    assert_eq!(
        executor["tool_options"]["enabled_builtin_kinds"],
        json!(["CodeMaintainerRead", "TerminalController"])
    );
    assert!(normalized_team_executor_context(
        &metadata,
        "agent-team-executor-delivery-1",
        Some(&project_resource()),
    )
    .is_err());
    let mut survey = metadata.clone();
    survey["agent_team_execution"]["execution_plan"]["builtin_capabilities"] =
        json!(["requirement_survey_read", "requirement_survey_write"]);
    let survey = normalized_team_executor_context(&survey, "agent-team-executor-delivery-1", None)
        .unwrap()
        .unwrap();
    assert_eq!(
        survey["tool_options"]["enabled_builtin_kinds"],
        json!(["RequirementSurveyRead", "RequirementSurveyWrite"])
    );
    let mut escalated = metadata;
    escalated["agent_team_execution"]["execution_plan"]["builtin_capabilities"] =
        json!(["project_read", "admin"]);
    assert!(
        normalized_team_executor_context(&escalated, "agent-team-executor-delivery-1", None,)
            .is_err()
    );
}

#[test]
fn executor_context_projects_only_frozen_plugin_selections() {
    let mut metadata = team_executor_context();
    metadata["agent_team_execution"]["execution_plan"]["plugins"] = json!([{
        "installationId": "installation-1",
        "pluginId": "document-plugin",
        "componentKey": "document-mcp",
        "componentRevision": "revision-1",
        "releaseId": "release-1",
        "releaseDigest": format!("sha256:{}", "a".repeat(64)),
        "manifestVersion": "1.2.3",
        "displayName": "Document Tools",
        "reason": "生成验收文档",
    }]);

    let executor =
        normalized_team_executor_context(&metadata, "agent-team-executor-delivery-1", None)
            .unwrap()
            .unwrap();

    assert_eq!(
        executor["tool_options"]["plugin_hints"],
        json!([{
            "plugin_key": "document-plugin",
            "component_key": "document-mcp",
            "reason": "生成验收文档",
        }])
    );
    assert_eq!(
        executor["tool_options"]["plugin_release_snapshots"][0]["release_id"],
        "release-1"
    );
    assert_eq!(
        executor["execution_plan"]["plugins"][0]["installation_id"],
        "installation-1"
    );

    let mut forged = metadata;
    forged["agent_team_execution"]["execution_plan"]["plugins"][0]["path"] =
        json!("C:\\private\\plugin.exe");
    assert!(
        normalized_team_executor_context(&forged, "agent-team-executor-delivery-1", None,).is_err()
    );
}

#[tokio::test]
async fn only_a_bound_team_executor_turn_uses_task_execution() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(request(
            "create-user-project-conversation",
            HostCommand::CreateConversation(CreateConversationCommand {
                conversation_id: "project-conversation-1".into(),
                owner_user_id: "user-1".into(),
                title: "Project conversation".into(),
                resource: Some(project_resource()),
            }),
        ))
        .await
        .expect("create the user-facing project conversation");
    let conversation_id = "agent-team-executor-delivery-1";
    let mut turn = start_turn(1);
    turn.conversation_id = conversation_id.into();
    turn.conversation_if_missing = Some(CreateConversationCommand {
        conversation_id: conversation_id.into(),
        owner_user_id: "user-1".into(),
        title: "Executor".into(),
        resource: None,
    });
    turn.message_metadata = team_executor_context();
    let started = runtime
        .try_handle(request(
            "start-executor-turn",
            HostCommand::StartConversationTurn(turn),
        ))
        .await
        .expect("start executor Turn");
    let HostResult::ConversationTurnStarted { result } = started else {
        panic!("unexpected start result")
    };
    assert_eq!(result.run.profile_key, "task_execution");
    assert_eq!(result.run.input["project_id"], "project-1");
    assert_eq!(result.run.input["prompt"], "hello");
    assert_eq!(
        result.run.input["tool_options"]["enabled_builtin_kinds"],
        json!(["CodeMaintainerRead", "TerminalController"])
    );
    assert_eq!(
        result.run.input["team_executor_context"]["todo_id"],
        "todo-1"
    );
    assert_eq!(result.run.input["todo_id"], "todo-1");
    assert_eq!(
        result.run.input["memory_thread_id"],
        "client-agent:todo:todo-1"
    );
}

#[tokio::test]
async fn project_conversation_carries_its_display_name_into_memory_input() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(request(
            "create-project-conversation",
            HostCommand::CreateConversation(CreateConversationCommand {
                conversation_id: "project-conversation-1".into(),
                owner_user_id: "user-1".into(),
                title: "红果逆向".into(),
                resource: Some(project_resource()),
            }),
        ))
        .await
        .expect("create project conversation");
    let mut turn = start_turn(1);
    turn.conversation_id = "project-conversation-1".into();
    turn.message_metadata = json!({
        "source": "native_main_chat",
        "agent_id": "jiguli",
        "agent_name": "叽咕狸",
        "project_name": "红果逆向",
    });

    let started = runtime
        .try_handle(request(
            "start-project-turn",
            HostCommand::StartConversationTurn(turn),
        ))
        .await
        .expect("start project Turn");
    let HostResult::ConversationTurnStarted { result } = started else {
        panic!("unexpected start result")
    };
    assert_eq!(result.run.input["project_id"], "project-1");
    assert_eq!(result.run.input["project_name"], "红果逆向");
}

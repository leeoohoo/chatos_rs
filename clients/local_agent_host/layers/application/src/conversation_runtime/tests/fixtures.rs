struct TranscriptFixture;

struct LiveTranscriptFixture {
    storage: Arc<SqliteClientStorage>,
    records: std::sync::Mutex<Vec<AuthoritativeConversationRecord>>,
    sync_during_read: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl LocalConversationTranscriptSource for LiveTranscriptFixture {
    async fn list_conversation_records(
        &self,
        tenant_id: &str,
        thread_id: &str,
        conversation_id: &str,
        _before_cursor: Option<&str>,
        _limit: u32,
    ) -> Result<AuthoritativeConversationPage, String> {
        assert_eq!(tenant_id, "user-1");
        assert_eq!(thread_id, "client-agent:agent:jiguli");
        assert_eq!(conversation_id, "conversation-1");
        // The HTTP response can reflect a snapshot taken before ingestion,
        // even if the outbox is marked synced by the time it arrives.
        let records = self.records.lock().unwrap().clone();
        if self
            .sync_during_read
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            while let Some(claimed) = self
                .storage
                .claim_next_memory_record("user-1", "sync", 20_000, 30_000)
                .await
                .unwrap()
            {
                self.storage
                    .complete_memory_record(
                        &claimed.source_id,
                        &claimed.record_id,
                        "sync",
                        claimed.version,
                        21_000,
                    )
                    .await
                    .unwrap();
            }
        }
        Ok(AuthoritativeConversationPage {
            records,
            next_before_cursor: Some("older".to_string()),
        })
    }
}

#[async_trait]
impl LocalConversationTranscriptSource for TranscriptFixture {
    async fn list_conversation_records(
        &self,
        tenant_id: &str,
        thread_id: &str,
        conversation_id: &str,
        before_cursor: Option<&str>,
        limit: u32,
    ) -> Result<AuthoritativeConversationPage, String> {
        assert_eq!(tenant_id, "user-1");
        assert_eq!(thread_id, "client-agent:agent:jiguli");
        assert_eq!(conversation_id, "conversation-1");
        assert_eq!(before_cursor, None);
        assert_eq!(limit, 10);
        Ok(AuthoritativeConversationPage {
            records: vec![
                AuthoritativeConversationRecord {
                    record_id: "remote-assistant".to_string(),
                    role: "assistant".to_string(),
                    content: "remote answer".to_string(),
                    structured_payload: None,
                    metadata: Some(json!({
                        "conversation_id": "conversation-1",
                        "conversation_turn_id": "turn-1"
                    })),
                    created_at: "2026-11-06T10:00:01Z".to_string(),
                },
                AuthoritativeConversationRecord {
                    record_id: "message-1".to_string(),
                    role: "user".to_string(),
                    content: "remote hello".to_string(),
                    structured_payload: None,
                    metadata: Some(json!({
                        "conversation_id": "conversation-1",
                        "conversation_turn_id": "turn-1",
                        "ui_message_metadata": {
                            "composer": "main_chat",
                            "client_marker": "authoritative"
                        },
                        "ui_attachments": [{
                            "attachment_id": "attachment-1",
                            "display_name": "brief.pdf",
                            "media_type": "application/pdf",
                            "byte_size": 128,
                            "sha256": "a".repeat(64),
                            "authorized_local_ref": "local-attachment:brief.pdf",
                            "metadata": {"origin": "memory"}
                        }]
                    })),
                    created_at: "2026-11-06T10:00:00Z".to_string(),
                },
            ],
            next_before_cursor: Some("cursor-2".to_string()),
        })
    }
}

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn create_conversation() -> CreateConversationCommand {
    CreateConversationCommand {
        conversation_id: "conversation-1".to_string(),
        owner_user_id: "user-1".to_string(),
        title: "Local conversation".to_string(),
        resource: Some(LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id: JIGULI_AGENT_ID.to_string(),
        }),
    }
}

fn start_turn(expected_conversation_version: u64) -> StartConversationTurnCommand {
    StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-1".to_string(),
        expected_conversation_version,
        conversation_if_missing: None,
        turn_id: "turn-1".to_string(),
        message_id: "message-1".to_string(),
        run_id: "run-1".to_string(),
        message: "hello".to_string(),
        message_metadata: json!({
            "source": "composer",
            "agent_id": "jiguli",
            "agent_name": "叽咕狸",
            "explicit_capability_selection": {
                "kind": "plugin",
                "plugin_key": "document-plugin",
                "component_key": "document-mcp",
                "display_name": "Document"
            }
        }),
        attachments: vec![LocalConversationAttachmentSpec {
            attachment_id: "attachment-1".to_string(),
            display_name: "brief.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            byte_size: 42,
            sha256: "a".repeat(64),
            authorized_local_ref: "local-attachment:authority-1".to_string(),
            metadata: json!({"page_count": 1}),
        }],
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    }
}

fn agent_context() -> serde_json::Value {
    json!({
        "source": "native_agent_direct",
        "agent_execution_context": {
            "schema_version": 1,
            "room_id": "room-1",
            "agent_id": "agent-1",
            "agent_name": "工程 Agent",
            "role_prompt": "负责工程实现\n保持证据可追踪。",
            "profession_key": "software_engineer",
            "profession_label": "软件工程师",
            "profession_skill_ref": "AS-profession-software_engineer-123456789abc",
            "profession_skill_sha256": "a".repeat(64),
            "profession_skill_markdown": "# 工作流\n先核实，再实现。",
            "default_plugin_ids": [],
            "default_skill_ids": ["local.project.list"],
        }
    })
}

fn team_agent_context() -> serde_json::Value {
    json!({
        "source": "native_agent_team",
        "agent_execution_context": {
            "schema_version": 2,
            "room_id": "team-1",
            "agent_id": "agent-1",
            "agent_name": "工程 Agent",
            "role_prompt": "负责工程实现。",
            "profession_key": "software_engineer",
            "profession_label": "软件工程师",
            "profession_skill_ref": "AS-profession-software_engineer-123456789abc",
            "profession_skill_sha256": "a".repeat(64),
            "profession_skill_markdown": "# 工作流\n先核实，再实现。",
            "default_plugin_ids": [],
            "default_skill_ids": ["local.project.list"],
            "project_id": "project-1",
            "team_name": "Alpha 团队",
            "team_goal": "交付 Alpha",
            "team_role": "前端工程师",
            "team_responsibility": "负责 Windows UI",
            "is_project_manager": false,
        }
    })
}

fn team_executor_context() -> serde_json::Value {
    let mut metadata = team_agent_context();
    metadata["agent_team_execution"] = json!({
        "schema_version": 1,
        "delivery_lane": "executor",
        "trigger_kind": "todo",
        "delivery_id": "delivery-1",
        "todo_id": "todo-1",
        "team_id": "team-1",
        "agent_id": "agent-1",
        "project_id": "project-1",
        "execution_plan": {
            "requires_execution": true,
            "builtin_capabilities": ["project_read", "terminal"],
            "plugins": [],
        }
    });
    metadata
}

fn project_resource() -> LocalConversationResourceBinding {
    LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Project,
        resource_id: "project-1".into(),
    }
}

impl LocalAgentRuntime {
    /// Trusted Task-contract lookup. Native callers use the owner-scoped IPC command.
    #[doc(hidden)]
    pub async fn conversation_for_host_worker(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .get_conversation(owner_user_id, conversation_id)
            .await?)
    }

    /// Returns only recent attachment metadata and opaque authorization references.
    /// Main Chat uses this catalog to select the minimum inputs for a Task; attachment
    /// bytes and base64 are never copied into the model-owned Task arguments.
    pub async fn recent_conversation_attachments(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Vec<LocalConversationAttachmentRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .list_recent_conversation_attachments(owner_user_id, conversation_id, 100)
            .await?)
    }

    /// Resolves one opaque reference inside the source conversation boundary.
    pub async fn conversation_attachment_by_ref(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        authorized_local_ref: &str,
    ) -> Result<Option<LocalConversationAttachmentRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .get_conversation_attachment(owner_user_id, conversation_id, authorized_local_ref)
            .await?)
    }

    pub(super) async fn handle_conversation_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::CreateConversation(command) => Ok(HostResult::Conversation {
                conversation: self.create_conversation(idempotency, command).await?,
            }),
            HostCommand::GetConversation(command) => Ok(HostResult::Conversation {
                conversation: self
                    .store
                    .get_conversation(&command.owner_user_id, &command.conversation_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.conversation_id))?,
            }),
            HostCommand::GetConversationContext(command) => Ok(HostResult::Conversation {
                conversation: self
                    .store
                    .get_conversation_context(&command.owner_user_id, &command.conversation_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.conversation_id))?,
            }),
            HostCommand::GetConversationAttachment(command) => {
                Ok(HostResult::ConversationAttachment {
                    attachment: self
                        .store
                        .get_conversation_attachment(
                            &command.owner_user_id,
                            &command.conversation_id,
                            &command.authorized_local_ref,
                        )
                        .await?,
                })
            }
            HostCommand::GetConversationHistory(command) => Ok(HostResult::ConversationHistory {
                page: Box::new(self.authoritative_conversation_history(command).await?),
            }),
            HostCommand::ListConversations(command) => Ok(HostResult::Conversations {
                page: self
                    .store
                    .list_conversations(
                        &command.owner_user_id,
                        command.before_updated_at_unix_ms,
                        command.before_conversation_id.as_deref(),
                        command.limit,
                    )
                    .await?,
            }),
            HostCommand::StartConversationTurn(command) => {
                let result = self.start_conversation_turn(idempotency, command).await?;
                Ok(HostResult::ConversationTurnStarted {
                    result: Box::new(result),
                })
            }
            HostCommand::GuideConversationTurn(command) => {
                let result = self
                    .store
                    .guide_conversation_turn(idempotency, &command, &new_event_id(), self.now()?)
                    .await?;
                Ok(HostResult::ConversationTurnUpdated {
                    result: Box::new(result),
                })
            }
            HostCommand::ResumeConversationTurn(command) => {
                let result = self.resume_conversation_turn(idempotency, command).await?;
                Ok(HostResult::ConversationTurnUpdated {
                    result: Box::new(result),
                })
            }
            HostCommand::CancelConversationTurn(command) => {
                let result = self.cancel_conversation_turn(idempotency, command).await?;
                Ok(HostResult::ConversationTurnUpdated {
                    result: Box::new(result),
                })
            }
            _ => unreachable!("non-conversation command routed to conversation runtime"),
        }
    }

    async fn create_conversation(
        &self,
        idempotency: &IdempotentCommand,
        command: CreateConversationCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationDetail, LocalAgentRuntimeError> {
        Ok(self
            .store
            .create_conversation(idempotency, &command, self.now()?)
            .await?)
    }

    async fn start_conversation_turn(
        &self,
        idempotency: &IdempotentCommand,
        command: StartConversationTurnCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnStart, LocalAgentRuntimeError>
    {
        let now = self.now()?;
        let existing_conversation = self
            .store
            .get_conversation(&command.owner_user_id, &command.conversation_id)
            .await?;
        let conversation = match (
            existing_conversation,
            command.conversation_if_missing.as_ref(),
        ) {
            (Some(conversation), _) => conversation,
            (None, Some(seed)) => LocalConversationDetail {
                conversation: LocalConversationRecord {
                    conversation_id: seed.conversation_id.clone(),
                    owner_user_id: seed.owner_user_id.clone(),
                    title: seed.title.trim().to_string(),
                    resource: seed.resource.clone(),
                    version: 1,
                    created_at_unix_ms: now,
                    updated_at_unix_ms: now,
                },
                turns: Vec::new(),
                messages: Vec::new(),
                attachments: Vec::new(),
            },
            (None, None) => {
                return Err(ClientStorageError::NotFound(command.conversation_id.clone()).into())
            }
        };
        let runtime_settings = self
            .store
            .get_conversation_runtime_settings(&command.owner_user_id, &command.conversation_id)
            .await?;
        if let Some(settings) = &runtime_settings {
            if settings.selected_model_config_ref != command.model_config_ref
                || settings.selected_model_config_revision != command.model_config_revision
            {
                return Err(LocalAgentRuntimeError::InvalidRequest(
                    "Turn model does not match the current conversation runtime settings"
                        .to_string(),
                ));
            }
        }
        let explicit_capability_selection =
            normalized_explicit_capability_selection(&command.message_metadata)?;
        let team_executor_context = normalized_team_executor_context(
            &command.message_metadata,
            &command.conversation_id,
            conversation.conversation.resource.as_ref(),
        )?;
        let agent_execution_context = normalized_agent_execution_context(
            &command.message_metadata,
            &command.conversation_id,
            conversation.conversation.resource.as_ref(),
            team_executor_context.as_ref(),
        )?;
        if team_executor_context.is_some() && explicit_capability_selection.is_some() {
            return Err(invalid_agent_context(
                "executor turns must not contain a Main Chat capability selection",
            ));
        }
        let conversation_agent_identity =
            if agent_execution_context.is_none() && team_executor_context.is_none() {
                Some(normalized_conversation_agent_identity(
                    &command.message_metadata,
                    conversation.conversation.resource.as_ref(),
                )?)
            } else {
                None
            };
        let mut input = json!({
            "conversation_id": &command.conversation_id,
            "conversation_title": &conversation.conversation.title,
            "conversation_resource": &conversation.conversation.resource,
            "turn_id": &command.turn_id,
            "message_id": &command.message_id,
            "message": &command.message,
            "ui_message_metadata": &command.message_metadata,
            "attachments": &command.attachments,
            "runtime_settings": runtime_settings.as_ref().map(|settings| json!({
                "version": settings.version,
                "selected_thinking_level": settings.selected_thinking_level,
                "remote_connection_id": settings.remote_connection_id,
                "reasoning_enabled": settings.reasoning_enabled,
            })),
        });
        if let Some((agent_id, agent_name)) = conversation_agent_identity {
            let object = input
                .as_object_mut()
                .expect("conversation Run input is an object");
            object.insert("agent_id".to_string(), json!(agent_id));
            if let Some(agent_name) = agent_name {
                object.insert("agent_name".to_string(), json!(agent_name));
            }
        }
        if let Some(LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Project,
            resource_id,
        }) = conversation.conversation.resource.as_ref()
        {
            let object = input
                .as_object_mut()
                .expect("conversation Run input is an object");
            object.insert("project_id".to_string(), json!(resource_id));
            let metadata = command
                .message_metadata
                .as_object()
                .expect("validated message metadata is an object");
            if metadata.get("project_name").is_some() {
                object.insert(
                    "project_name".to_string(),
                    json!(agent_context_string(metadata, "project_name", 240, false)?),
                );
            }
        }
        if let Some(selection) = explicit_capability_selection {
            input
                .as_object_mut()
                .expect("conversation Run input is an object")
                .insert("explicit_capability_selection".to_string(), selection);
        }
        if let Some(context) = agent_execution_context {
            input
                .as_object_mut()
                .expect("conversation Run input is an object")
                .insert("agent_execution_context".to_string(), context);
        }
        let profile_key = if let Some(context) = team_executor_context {
            let object = input
                .as_object_mut()
                .expect("conversation Run input is an object");
            let todo_id = context
                .get("todo_id")
                .and_then(serde_json::Value::as_str)
                .expect("normalized executor context has todo_id");
            object.insert("todo_id".to_string(), json!(todo_id));
            object.insert(
                "memory_thread_id".to_string(),
                json!(format!("client-agent:todo:{todo_id}")),
            );
            object.insert("prompt".to_string(), json!(&command.message));
            object.insert("owned_paths".to_string(), json!([]));
            object.insert(
                "tool_options".to_string(),
                context
                    .get("tool_options")
                    .cloned()
                    .expect("normalized executor context has tool_options"),
            );
            object.insert(
                "project_id".to_string(),
                context
                    .get("project_id")
                    .cloned()
                    .expect("normalized executor context has project_id"),
            );
            object.insert("team_executor_context".to_string(), context);
            "task_execution"
        } else {
            "main_chat"
        };
        let run = LocalAgentRunRecord {
            run_id: command.run_id.clone(),
            owner_user_id: conversation.conversation.owner_user_id,
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: command.turn_id.clone(),
            profile_key: profile_key.to_string(),
            model_config_ref: command.model_config_ref.clone(),
            model_config_revision: command.model_config_revision.clone(),
            capability_policy_revision: command.capability_policy_revision.clone(),
            input,
            status: LocalAgentRunStatus::Queued,
            iteration: 0,
            model_attempt: 1,
            max_iterations: command.max_iterations,
            version: 1,
            claim_token: None,
            claim_until_unix_ms: None,
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: serde_json::Value::Null,
            continuation_input: None,
            terminal_outcome: None,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        Ok(self
            .store
            .start_conversation_turn(idempotency, &command, &run, &new_event_id(), now)
            .await?)
    }

    async fn resume_conversation_turn(
        &self,
        idempotency: &IdempotentCommand,
        command: ResumeConversationTurnCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnUpdate, LocalAgentRuntimeError>
    {
        let continuation_input = json!({
            "type": "resume",
            "reason": &command.reason,
            "input": {
                "message": &command.message,
                "attachments": &command.attachments,
            }
        });
        Ok(self
            .store
            .resume_conversation_turn(
                idempotency,
                &command,
                &continuation_input,
                &new_event_id(),
                self.now()?,
            )
            .await?)
    }

    async fn cancel_conversation_turn(
        &self,
        idempotency: &IdempotentCommand,
        command: CancelConversationTurnCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnUpdate, LocalAgentRuntimeError>
    {
        Ok(self
            .store
            .cancel_conversation_turn(idempotency, &command, &new_event_id(), self.now()?)
            .await?)
    }
}

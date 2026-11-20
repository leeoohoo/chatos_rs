#[allow(clippy::too_many_arguments)]
async fn project_user_message_to_memory(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    conversation_id: &str,
    turn_id: &str,
    message_id: &str,
    message: &str,
    message_metadata: &Value,
    attachments: &[LocalConversationAttachmentSpec],
    created_at_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    project_conversation_message_to_memory(
        connection,
        run,
        conversation_id,
        turn_id,
        message_id,
        "user",
        &Value::String(message.to_string()),
        message_metadata,
        attachments,
        "conversation_continuation",
        "pending",
        false,
        false,
        created_at_unix_ms,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn project_conversation_message_to_memory(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    conversation_id: &str,
    turn_id: &str,
    message_id: &str,
    role: &str,
    content: &Value,
    message_metadata: &Value,
    attachments: &[LocalConversationAttachmentSpec],
    message_source: &str,
    summary_status: &str,
    structured_content: bool,
    mutable: bool,
    created_at_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let source_id = sqlx::query_scalar::<_, String>(
        "SELECT source_id FROM local_memory_projection_config WHERE owner_user_id = ?",
    )
    .bind(&run.owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(source_id) = source_id else {
        return Ok(());
    };
    let (agent_id, agent_name) =
        conversation_projection_agent_identity(connection, run, conversation_id).await?;
    let project_id = consistent_run_identity(&run.input, "project_id")?;
    let project_name = consistent_run_identity(&run.input, "project_name")?;
    let is_task_execution = run.profile_key == "task_execution";
    let thread_kind = if is_task_execution {
        if consistent_run_identity(&run.input, "todo_id")?.is_some() {
            "todo_execution"
        } else {
            "agent_task"
        }
    } else if run.profile_key == "main_chat" && agent_id == "jiguli" {
        "main_chat"
    } else {
        "agent_conversation"
    };
    let thread_project_aggregation_id = is_task_execution.then(|| project_id.clone()).flatten();
    let thread_id = if run.profile_key == "task_execution" {
        run.input
            .get("memory_thread_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(run.owner_entity_id.as_str())
            .to_string()
    } else {
        format!("client-agent:agent:{agent_id}")
    };
    let created_at = DateTime::<Utc>::from_timestamp_millis(created_at_unix_ms)
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "conversation Memory projection timestamp is invalid".to_string(),
            )
        })?
        .to_rfc3339();
    let mut metadata = message_metadata.as_object().cloned().unwrap_or_default();
    metadata.insert("tenant_id".to_string(), json!(run.owner_user_id));
    metadata.insert("conversation_id".to_string(), json!(conversation_id));
    metadata.insert(
        "conversation_title".to_string(),
        run.input
            .get("conversation_title")
            .cloned()
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "conversation_resource".to_string(),
        run.input
            .get("conversation_resource")
            .cloned()
            .unwrap_or(Value::Null),
    );
    metadata.insert("profile_key".to_string(), json!(run.profile_key));
    metadata.insert("agent_id".to_string(), json!(agent_id));
    metadata.insert("agent_name".to_string(), json!(agent_name));
    metadata.insert("project_id".to_string(), json!(project_id));
    metadata.insert("project_name".to_string(), json!(project_name));
    metadata.insert("memory_project_id".to_string(), json!(project_id));
    metadata.insert(
        "thread_project_aggregation_id".to_string(),
        json!(thread_project_aggregation_id),
    );
    metadata.insert("thread_kind".to_string(), json!(thread_kind));
    metadata.insert("run_id".to_string(), json!(run.run_id));
    metadata.insert(
        "owner_entity_type".to_string(),
        json!(run.owner_entity_type),
    );
    metadata.insert("owner_entity_id".to_string(), json!(run.owner_entity_id));
    metadata.insert("ui_message_id".to_string(), json!(message_id));
    metadata.insert("ui_message_metadata".to_string(), message_metadata.clone());
    metadata.insert("ui_attachments".to_string(), json!(attachments));
    if structured_content {
        metadata.insert("ui_structured_content".to_string(), json!(true));
    }
    if mutable {
        metadata.insert("memory_projection_mutable".to_string(), json!(true));
    }
    let display_content = content
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            content
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| content.to_string());
    let payload = json!({
        "conversation_id": thread_id,
        "conversation_turn_id": turn_id,
        "message_id": message_id,
        "role": role,
        "content": display_content,
        "structured_payload": if structured_content { Some(content.clone()) } else { None },
        "metadata": metadata,
        "message_mode": if run.profile_key == "task_execution" { "task_run" } else { "main_chat" },
        "message_source": message_source,
        "summary_status": summary_status,
        "created_at": created_at,
    });
    memory_outbox_store::enqueue_on(
        connection,
        message_id,
        &run.owner_user_id,
        &source_id,
        payload
            .get("conversation_id")
            .and_then(Value::as_str)
            .expect("projection thread id"),
        &payload,
        created_at_unix_ms,
    )
    .await?;
    Ok(())
}

async fn conversation_projection_agent_identity(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    conversation_id: &str,
) -> Result<(String, Option<String>), ClientStorageError> {
    let agent_name = consistent_run_identity(&run.input, "agent_name")?;
    if let Some(agent_id) = consistent_run_identity(&run.input, "agent_id")? {
        return Ok((agent_id, agent_name));
    }
    if run.profile_key != "main_chat" {
        return Err(ClientStorageError::InvalidState(
            "conversation Memory projection requires agent_id".to_string(),
        ));
    }

    // Runs created before Agent-bound conversations stored their identity in the
    // durable conversation resource, but not in input_json. Recovery can still
    // project their terminal message without weakening identity checks for new
    // Runs: Contact resource IDs are Agent IDs, while project chat is Jiguli.
    let resource = sqlx::query(
        "SELECT resource_kind, resource_id, title FROM local_conversations \
         WHERE conversation_id = ? AND owner_user_id = ?",
    )
    .bind(conversation_id)
    .bind(&run.owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(resource) = resource else {
        return Err(ClientStorageError::InvalidState(
            "conversation Memory projection requires agent_id".to_string(),
        ));
    };
    let resource_kind: Option<String> = resource.try_get("resource_kind").db()?;
    let resource_id: Option<String> = resource.try_get("resource_id").db()?;
    match (resource_kind.as_deref(), resource_id) {
        (Some("contact"), Some(agent_id)) if !agent_id.trim().is_empty() => {
            let title: String = resource.try_get("title").db()?;
            let agent_name = (!title.trim().is_empty()).then_some(title);
            Ok((agent_id, agent_name))
        }
        (Some("project"), Some(_)) => Ok(("jiguli".to_string(), None)),
        _ => Err(ClientStorageError::InvalidState(
            "conversation Memory projection requires agent_id".to_string(),
        )),
    }
}

fn consistent_run_identity(input: &Value, key: &str) -> Result<Option<String>, ClientStorageError> {
    let values = [
        input.get(key),
        input
            .get("agent_execution_context")
            .and_then(|value| value.get(key)),
        input
            .get("agent_team_execution")
            .and_then(|value| value.get(key)),
        input
            .get("team_executor_context")
            .and_then(|value| value.get(key)),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>();
    let Some(value) = values.first() else {
        return Ok(None);
    };
    if values.iter().any(|candidate| candidate != value) {
        return Err(ClientStorageError::InvalidState(format!(
            "conversation Memory projection has conflicting {key} values"
        )));
    }
    Ok(Some((*value).to_string()))
}

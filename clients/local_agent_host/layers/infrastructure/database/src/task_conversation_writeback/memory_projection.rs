#[allow(clippy::too_many_arguments)]
async fn project_program_message_to_memory(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    source_run_id: &str,
    source_run_input: &Value,
    conversation_id: &str,
    turn_id: &str,
    message_id: &str,
    content: &Value,
    metadata: &Value,
    created_at_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let source_id = sqlx::query_scalar::<_, String>(
        "SELECT source_id FROM local_memory_projection_config WHERE owner_user_id = ?",
    )
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(source_id) = source_id else {
        return Ok(());
    };
    let agent_id = memory_identity_value(source_run_input, "agent_id").ok_or_else(|| {
        ClientStorageError::InvalidState(
            "program conversation Memory projection requires agent_id".to_string(),
        )
    })?;
    let agent_name = memory_identity_value(source_run_input, "agent_name");
    let project_id = memory_identity_value(source_run_input, "project_id");
    let project_name = memory_identity_value(source_run_input, "project_name");
    let thread_kind = if agent_id == "jiguli" {
        "main_chat"
    } else {
        "agent_conversation"
    };
    let thread_id = format!("client-agent:agent:{agent_id}");
    let mut remote_metadata = metadata.as_object().cloned().unwrap_or_default();
    remote_metadata.insert("tenant_id".to_string(), json!(owner_user_id));
    remote_metadata.insert("conversation_id".to_string(), json!(conversation_id));
    remote_metadata.insert("run_id".to_string(), json!(source_run_id));
    remote_metadata.insert("agent_id".to_string(), json!(agent_id));
    remote_metadata.insert("agent_name".to_string(), json!(agent_name));
    remote_metadata.insert("project_id".to_string(), json!(project_id));
    remote_metadata.insert("project_name".to_string(), json!(project_name));
    remote_metadata.insert("memory_project_id".to_string(), json!(project_id));
    remote_metadata.insert("thread_project_aggregation_id".to_string(), Value::Null);
    remote_metadata.insert("thread_kind".to_string(), json!(thread_kind));
    remote_metadata.insert("ui_message_id".to_string(), json!(message_id));
    remote_metadata.insert("ui_structured_content".to_string(), json!(true));
    remote_metadata.insert("memory_projection_mutable".to_string(), json!(true));
    let display_text = content
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            content
                .get("delivery_text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            content
                .get("summary")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| content.to_string());
    let created_at = DateTime::<Utc>::from_timestamp_millis(created_at_unix_ms)
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "program conversation Memory projection timestamp is invalid".to_string(),
            )
        })?
        .to_rfc3339();
    let payload = json!({
        "conversation_id": thread_id,
        "conversation_turn_id": turn_id,
        "message_id": message_id,
        "role": "assistant",
        "content": display_text,
        "structured_payload": content,
        "metadata": remote_metadata,
        "message_mode": "main_chat",
        "message_source": "program_projection",
        "summary_status": "ignored",
        "created_at": created_at,
    });
    memory_outbox_store::enqueue_on(
        connection,
        message_id,
        owner_user_id,
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

fn memory_identity_value<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| {
            input
                .pointer(&format!("/agent_execution_context/{key}"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            input
                .pointer(&format!("/agent_team_execution/{key}"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            input
                .pointer(&format!("/team_executor_context/{key}"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn unix_ms_rfc3339(value: i64) -> Value {
    DateTime::<Utc>::from_timestamp_millis(value)
        .map(|value| Value::String(value.to_rfc3339()))
        .unwrap_or(Value::Null)
}

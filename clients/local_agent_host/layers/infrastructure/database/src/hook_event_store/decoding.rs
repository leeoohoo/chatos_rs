async fn fetch_event(
    connection: &mut sqlx::SqliteConnection,
    event_id: &str,
) -> Result<Option<LocalHookEventRecord>, ClientStorageError> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "{EVENT_SELECT} WHERE event_id = ?"
    )))
    .bind(event_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_event)
    .transpose()
}

fn decode_event(row: SqliteRow) -> Result<LocalHookEventRecord, ClientStorageError> {
    Ok(LocalHookEventRecord {
        spec: LocalHookEventSpec {
            event_id: row.try_get("event_id").db()?,
            owner_user_id: row.try_get("owner_user_id").db()?,
            event_type: row.try_get("event_type").db()?,
            subject_id: row.try_get("subject_id").db()?,
            sequence: u64::try_from(row.try_get::<i64, _>("sequence").db()?).map_err(|_| {
                ClientStorageError::InvalidState("invalid Hook event sequence".to_string())
            })?,
            payload_sha256: row.try_get("payload_sha256").db()?,
            payload_json: row.try_get("payload_json").db()?,
            matcher_value: row.try_get("matcher_value").db()?,
            active_bundle_ids: serde_json::from_str(
                &row.try_get::<String, _>("active_bundle_ids_json").db()?,
            )?,
        },
        status: row.try_get("status").db()?,
        additional_context: row.try_get("additional_context").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

async fn fetch_invocation(
    connection: &mut sqlx::SqliteConnection,
    invocation_id: &str,
) -> Result<Option<LocalHookInvocationRecord>, ClientStorageError> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "{INVOCATION_SELECT} WHERE invocation_id = ?"
    )))
    .bind(invocation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_invocation)
    .transpose()
}

fn decode_invocation(row: SqliteRow) -> Result<LocalHookInvocationRecord, ClientStorageError> {
    Ok(LocalHookInvocationRecord {
        spec: LocalHookInvocationSpec {
            invocation_id: row.try_get("invocation_id").db()?,
            event_id: row.try_get("event_id").db()?,
            bundle_id: row.try_get("bundle_id").db()?,
            hook_id: row.try_get("hook_id").db()?,
            is_async: row.try_get("is_async").db()?,
        },
        attempt: u32::try_from(row.try_get::<i64, _>("attempt").db()?).map_err(|_| {
            ClientStorageError::InvalidState("invalid Hook invocation attempt".to_string())
        })?,
        status: row.try_get("status").db()?,
        exit_code: row.try_get("exit_code").db()?,
        timed_out: row.try_get("timed_out").db()?,
        duration_ms: to_u64(row.try_get("duration_ms").db()?, "Hook duration")?,
        stdout_sha256: row.try_get("stdout_sha256").db()?,
        stdout_bytes: to_u64(row.try_get("stdout_bytes").db()?, "Hook stdout size")?,
        stdout_overflow: row.try_get("stdout_overflow").db()?,
        stderr_sha256: row.try_get("stderr_sha256").db()?,
        stderr_bytes: to_u64(row.try_get("stderr_bytes").db()?, "Hook stderr size")?,
        stderr_overflow: row.try_get("stderr_overflow").db()?,
        stderr_summary: row.try_get("stderr_summary").db()?,
        additional_context: row.try_get("additional_context").db()?,
        async_delivery_key: row.try_get("async_delivery_key").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn decode_invocation_diagnostic(
    row: SqliteRow,
) -> Result<LocalPluginHookInvocationDiagnostic, ClientStorageError> {
    Ok(LocalPluginHookInvocationDiagnostic {
        invocation_id: row.try_get("invocation_id").db()?,
        bundle_id: row.try_get("bundle_id").db()?,
        hook_id: row.try_get("hook_id").db()?,
        event_type: row.try_get("event_type").db()?,
        is_async: row.try_get("is_async").db()?,
        attempt: u32::try_from(row.try_get::<i64, _>("attempt").db()?).map_err(|_| {
            ClientStorageError::InvalidState("invalid Hook invocation attempt".to_string())
        })?,
        status: row.try_get("status").db()?,
        exit_code: row.try_get("exit_code").db()?,
        timed_out: row.try_get("timed_out").db()?,
        duration_ms: to_u64(row.try_get("duration_ms").db()?, "Hook duration")?,
        stdout_bytes: to_u64(row.try_get("stdout_bytes").db()?, "Hook stdout size")?,
        stdout_overflow: row.try_get("stdout_overflow").db()?,
        stderr_bytes: to_u64(row.try_get("stderr_bytes").db()?, "Hook stderr size")?,
        stderr_overflow: row.try_get("stderr_overflow").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

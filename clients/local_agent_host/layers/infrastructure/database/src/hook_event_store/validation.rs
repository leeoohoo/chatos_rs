impl SqliteClientStorage {
    pub(super) async fn prune_hook_output_blobs(
        &self,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            let expired: Vec<String> = sqlx::query_scalar(
                "SELECT storage_ref FROM local_hook_output_blobs WHERE expires_at_unix_ms <= ?",
            )
            .bind(now_unix_ms)
            .fetch_all(&mut *connection)
            .await
            .db()?;
            sqlx::query("DELETE FROM local_hook_output_blobs WHERE expires_at_unix_ms <= ?")
                .bind(now_unix_ms)
                .execute(&mut *connection)
                .await
                .db()?;
            Ok(expired)
        }
        .await;
        let expired = Self::finish_write(&mut connection, result).await?;
        drop(connection);
        cleanup_expired_blobs(&self.hook_output_root, &expired).await;
        let retained: Vec<String> =
            sqlx::query_scalar("SELECT storage_ref FROM local_hook_output_blobs")
                .fetch_all(&self.pool)
                .await
                .db()?;
        cleanup_unreferenced_output_files(self.hook_output_root.clone(), retained).await
    }
}

fn validate_event(event: &LocalHookEventSpec) -> Result<(), ClientStorageError> {
    for (name, value) in [
        ("event_id", event.event_id.as_str()),
        ("owner_user_id", event.owner_user_id.as_str()),
        ("event_type", event.event_type.as_str()),
        ("subject_id", event.subject_id.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
        }
    }
    if event.sequence == 0 || !is_sha256(&event.payload_sha256) {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook event sequence or payload hash".to_string(),
        ));
    }
    if event.payload_json.is_empty()
        || event.payload_json.len() > 1024 * 1024
        || serde_json::from_str::<serde_json::Value>(&event.payload_json).is_err()
        || format!("{:x}", Sha256::digest(event.payload_json.as_bytes())) != event.payload_sha256
        || event.matcher_value.len() > 1_024
        || event.matcher_value.contains('\0')
    {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook event recovery material".to_string(),
        ));
    }
    let mut bundle_ids = event.active_bundle_ids.clone();
    bundle_ids.sort();
    bundle_ids.dedup();
    if bundle_ids != event.active_bundle_ids
        || bundle_ids.len() > chatos_local_agent_protocol::LOCAL_PLUGIN_HOOK_MAX_BUNDLES
        || bundle_ids.iter().any(|bundle_id| {
            bundle_id.trim().is_empty()
                || bundle_id.len() > 256
                || bundle_id.chars().any(char::is_control)
        })
    {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook event bundle snapshot".to_string(),
        ));
    }
    Ok(())
}

fn validate_invocation(invocation: &LocalHookInvocationSpec) -> Result<(), ClientStorageError> {
    for (name, value) in [
        ("invocation_id", invocation.invocation_id.as_str()),
        ("event_id", invocation.event_id.as_str()),
        ("bundle_id", invocation.bundle_id.as_str()),
        ("hook_id", invocation.hook_id.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
        }
    }
    Ok(())
}

fn validate_completion(
    completion: &LocalHookInvocationCompletion,
) -> Result<(), ClientStorageError> {
    if !matches!(
        completion.status.as_str(),
        "succeeded" | "failed" | "blocked"
    ) {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook invocation completion status".to_string(),
        ));
    }
    if completion
        .stdout_sha256
        .as_deref()
        .is_some_and(|value| !is_sha256(value))
        || completion
            .stderr_sha256
            .as_deref()
            .is_some_and(|value| !is_sha256(value))
        || completion
            .stderr_summary
            .as_ref()
            .map(String::len)
            .unwrap_or(0)
            > 4_096
        || completion
            .additional_context
            .as_ref()
            .map(String::len)
            .unwrap_or(0)
            > 256 * 1_024
    {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook invocation output".to_string(),
        ));
    }
    validate_spill(
        "stdout",
        completion.stdout_bytes,
        completion.stdout_sha256.as_deref(),
        completion.stdout_spill.as_ref(),
        512 * 1_024,
    )?;
    validate_spill(
        "stderr",
        completion.stderr_bytes,
        completion.stderr_sha256.as_deref(),
        completion.stderr_spill.as_ref(),
        64 * 1_024,
    )?;
    Ok(())
}

fn validate_spill(
    stream: &str,
    byte_count: u64,
    expected_sha256: Option<&str>,
    spill: Option<&chatos_local_agent_ports::LocalHookOutputSpill>,
    maximum_bytes: usize,
) -> Result<(), ClientStorageError> {
    let byte_count = usize::try_from(byte_count)
        .map_err(|_| ClientStorageError::InvalidState(format!("Hook {stream} size overflow")))?;
    if byte_count > maximum_bytes {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} exceeds its capture limit"
        )));
    }
    if byte_count != 0 && expected_sha256.is_none() {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} hash is missing"
        )));
    }
    let should_spill = byte_count > OUTPUT_SPILL_THRESHOLD_BYTES;
    if spill.is_some() != should_spill {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} spill does not match the inline threshold"
        )));
    }
    let Some(spill) = spill else { return Ok(()) };
    if spill.byte_count() != byte_count {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} spill size does not match"
        )));
    }
    let digest = format!("{:x}", Sha256::digest(spill.as_bytes()));
    if expected_sha256 != Some(digest.as_str()) {
        return Err(ClientStorageError::InvalidState(format!(
            "Hook {stream} spill hash does not match"
        )));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

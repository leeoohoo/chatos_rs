fn expected_revision<'a>(args: &'a Value, field: &str) -> Result<ExpectedRevision<'a>, String> {
    match args.get(field) {
        None => Ok(ExpectedRevision::Omitted),
        Some(Value::Null) => Ok(ExpectedRevision::Value(None)),
        Some(Value::String(value)) if is_sha256(value) => {
            Ok(ExpectedRevision::Value(Some(value.as_str())))
        }
        Some(Value::String(_)) => Err(format!(
            "{field} must be a lowercase 64-character SHA-256 value"
        )),
        Some(_) => Err(format!("{field} must be a SHA-256 string or null")),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn record_file_modification_outcome(
    tool: &str,
    ctx: &ToolContext<'_>,
    invocation: &Result<Value, String>,
) {
    let (outcome, success, changed, changed_target_count) = match invocation {
        Ok(value) => {
            let payload = chatos_mcp_runtime::structured_result_payload(value);
            let changed = payload
                .get("changed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let outcome = payload
                .get("outcome")
                .and_then(Value::as_str)
                .unwrap_or_else(|| FileModificationOutcome::from_changed(changed).as_str());
            let changed_target_count = payload
                .get("changed_target_count")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(changed));
            (outcome, true, changed, changed_target_count)
        }
        Err(error) => {
            let outcome = classify_file_modification_error(error);
            (outcome.as_str(), outcome.is_success(), false, 0)
        }
    };
    tracing::info!(
        event = "file_modification_outcome",
        source = "builtin_code_maintainer",
        tool,
        conversation_id = ctx.conversation_id,
        run_id = ctx.run_id,
        outcome,
        success,
        changed,
        changed_target_count,
        "file modification completed"
    );
}

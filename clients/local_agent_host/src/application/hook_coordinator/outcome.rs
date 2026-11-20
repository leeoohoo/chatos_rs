fn parse_hook_output(stdout: &[u8]) -> Option<HookOutput> {
    serde_json::from_value(parse_hook_json(stdout, false)?).ok()
}

fn parse_hook_json(stdout: &[u8], allow_standalone_line: bool) -> Option<Value> {
    serde_json::from_slice(stdout).ok().or_else(|| {
        allow_standalone_line.then_some(())?;
        let text = std::str::from_utf8(stdout).ok()?;
        text.lines().find_map(|line| {
            let candidate = line.trim();
            (candidate.starts_with('{') && candidate.ends_with('}'))
                .then(|| serde_json::from_str(candidate).ok())
                .flatten()
        })
    })
}

fn hook_diagnostic_summary(result: &LocalHookInvocationResult) -> Option<String> {
    if result.timed_out {
        return Some("Hook execution timed out".to_string());
    }
    if result.stdout_overflow {
        return Some("Hook stdout exceeded the 524288-byte capture limit".to_string());
    }
    if result.stderr_overflow {
        return Some("Hook stderr exceeded the 65536-byte capture limit".to_string());
    }
    (!result.stderr.is_empty()).then(|| safe_summary(&result.stderr))
}

fn merge_outcomes(
    event_type: &str,
    mut outcomes: Vec<InvocationOutcome>,
) -> Result<PersistedEventOutcome, String> {
    outcomes.sort_by(|left, right| {
        (&left.bundle_id, &left.hook_id).cmp(&(&right.bundle_id, &right.hook_id))
    });
    let blocked = outcomes.iter().any(|outcome| outcome.status == "blocked");
    let failed = outcomes.iter().any(|outcome| outcome.status == "failed");
    let block_reason = outcomes
        .iter()
        .find(|outcome| outcome.status == "blocked")
        .and_then(|outcome| outcome.block_reason.clone());
    let permission_decision = ["deny", "ask", "allow"].into_iter().find_map(|candidate| {
        outcomes
            .iter()
            .any(|outcome| outcome.permission_decision.as_deref() == Some(candidate))
            .then(|| candidate.to_string())
    });
    let decision_reason = outcomes
        .iter()
        .find(|outcome| {
            outcome.permission_decision.as_deref() == permission_decision.as_deref()
                && outcome.decision_reason.is_some()
        })
        .and_then(|outcome| outcome.decision_reason.clone());
    let updated_input = outcomes
        .iter()
        .rev()
        .find_map(|outcome| outcome.updated_input.clone());
    let mut sections = Vec::new();
    let mut total = 0_usize;
    for outcome in outcomes {
        let Some(context) = outcome.additional_context else {
            continue;
        };
        let section = format!(
            "[Plugin Hook: plugin={}, bundle={}, hook={}, event={}]\n{}",
            outcome.plugin_id, outcome.bundle_id, outcome.hook_id, event_type, context
        );
        total = total
            .checked_add(section.len())
            .and_then(|value| value.checked_add(2))
            .ok_or_else(|| "Hook event context size overflow".to_string())?;
        if total > MAXIMUM_EVENT_CONTEXT_BYTES {
            return Err(format!(
                "Hook event context exceeds {MAXIMUM_EVENT_CONTEXT_BYTES} bytes"
            ));
        }
        sections.push(section);
    }
    Ok(PersistedEventOutcome {
        event_type: event_type.to_string(),
        blocked,
        failed,
        block_reason,
        additional_context: (!sections.is_empty()).then(|| sections.join("\n\n")),
        permission_decision,
        decision_reason,
        updated_input,
    })
}

fn decode_invocation_output(encoded: Option<&str>) -> PersistedInvocationOutput {
    let Some(encoded) = encoded else {
        return PersistedInvocationOutput::default();
    };
    serde_json::from_str(encoded).unwrap_or_else(|_| PersistedInvocationOutput {
        additional_context: Some(encoded.to_string()),
        ..PersistedInvocationOutput::default()
    })
}

fn decode_persisted_outcome(
    event_type: &str,
    encoded: Option<String>,
) -> Result<PersistedEventOutcome, String> {
    let Some(encoded) = encoded else {
        return Ok(PersistedEventOutcome {
            event_type: event_type.to_string(),
            blocked: false,
            failed: false,
            block_reason: None,
            additional_context: None,
            permission_decision: None,
            decision_reason: None,
            updated_input: None,
        });
    };
    let outcome: PersistedEventOutcome = serde_json::from_str(&encoded)
        .map_err(|error| format!("decode persisted Hook outcome failed: {error}"))?;
    if outcome.event_type != event_type {
        return Err("persisted Hook outcome event type does not match".to_string());
    }
    Ok(outcome)
}

fn safe_summary(bytes: &[u8]) -> String {
    let mut summary = String::from_utf8_lossy(bytes).replace('\0', "�");
    if summary.len() > MAXIMUM_STDERR_SUMMARY_BYTES {
        let mut boundary = MAXIMUM_STDERR_SUMMARY_BYTES;
        while !summary.is_char_boundary(boundary) {
            boundary -= 1;
        }
        summary.truncate(boundary);
        summary.push('…');
    }
    summary
}

fn latest_compaction_marker(run: &LocalAgentRunRecord) -> Result<Option<String>, String> {
    for pointer in [
        "/response/response_output_items",
        "/response/request_input_items",
    ] {
        if let Some(item) = run
            .checkpoint
            .pointer(pointer)
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .rfind(|item| item.get("type").and_then(Value::as_str) == Some("compaction"))
            })
        {
            let encoded = serde_json::to_vec(item)
                .map_err(|error| format!("encode compaction marker failed: {error}"))?;
            return Ok(Some(sha256(&encoded)));
        }
    }
    Ok(None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StopBlockState {
    count: u64,
    reason_sha256: String,
}

fn stop_block_state(checkpoint: &Value) -> Option<StopBlockState> {
    let state = checkpoint.pointer("/continuation/stop_hook_block")?;
    let count = state.get("count")?.as_u64()?;
    let reason_sha256 = state.get("reason_sha256")?.as_str()?;
    (count > 0 && reason_sha256.len() == 64).then(|| StopBlockState {
        count,
        reason_sha256: reason_sha256.to_string(),
    })
}

fn stable_sequence(fields: &[&str]) -> u64 {
    let mut digest = Sha256::new();
    for field in fields {
        digest.update(field.as_bytes());
        digest.update([0]);
    }
    let bytes: [u8; 8] = digest.finalize()[..8]
        .try_into()
        .expect("SHA-256 prefix is eight bytes");
    (u64::from_be_bytes(bytes) & (i64::MAX as u64)).max(2)
}

fn stable_id(prefix: &str, fields: &[&str]) -> String {
    let mut digest = Sha256::new();
    for field in fields {
        digest.update(field.as_bytes());
        digest.update([0]);
    }
    format!("{prefix}-{:x}", digest.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

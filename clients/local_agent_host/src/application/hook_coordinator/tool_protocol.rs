fn tool_hook_subject(run_id: &str, call_id: &str) -> String {
    format!("{run_id}:{call_id}")
}

fn hook_session_id(run: &LocalAgentRunRecord) -> &str {
    run.input
        .get("conversation_id")
        .or_else(|| run.input.get("source_conversation_id"))
        .and_then(Value::as_str)
        .unwrap_or(&run.owner_entity_id)
}

fn tool_hook_payload(
    run: &LocalAgentRunRecord,
    event_type: &str,
    call_id: &str,
    tool_name: &str,
    arguments: &Value,
    response: Option<&Value>,
    cwd: String,
) -> Value {
    let session_id = hook_session_id(run);
    let mut payload = json!({
        "session_id": session_id,
        "hook_event_name": event_type,
        "tool_name": tool_name,
        "tool_input": arguments,
        "tool_use_id": call_id,
        "cwd": cwd,
    });
    if let Some(response) = response {
        payload["tool_response"] = hook_tool_response(response);
    }
    payload
}

fn hook_tool_response(response: &Value) -> Value {
    let original = serde_json::to_vec(response).unwrap_or_default();
    let mut sanitized = response.clone();
    redact_inline_binary(&mut sanitized);
    if serde_json::to_vec(&sanitized)
        .is_ok_and(|encoded| encoded.len() <= MAXIMUM_INLINE_HOOK_TOOL_RESPONSE_BYTES)
    {
        return sanitized;
    }

    let mut summary = serde_json::Map::new();
    for key in ["status", "error", "is_error", "made_progress"] {
        if let Some(value) = response.get(key) {
            summary.insert(key.to_string(), value.clone());
        }
    }
    summary.insert(
        "_chatos_hook_response".to_string(),
        json!({
            "omitted": true,
            "reason": "tool response exceeded the Hook inline payload limit",
            "original_bytes": original.len(),
            "sha256": sha256(&original),
        }),
    );
    Value::Object(summary)
}

fn redact_inline_binary(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(redact_inline_binary),
        Value::Object(object) => {
            let media_block = object
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| matches!(kind, "image" | "audio"));
            for (key, child) in object.iter_mut() {
                let Some(encoded) = child.as_str() else {
                    redact_inline_binary(child);
                    continue;
                };
                let inline_binary = (media_block && key == "data")
                    || matches!(key.as_str(), "base64" | "data_base64" | "blob")
                    || encoded.starts_with("data:image/")
                    || encoded.starts_with("data:audio/");
                if inline_binary && encoded.len() > MINIMUM_INLINE_BINARY_REDACTION_BYTES {
                    *child = Value::String(format!(
                        "[inline binary omitted from Hook payload: {} encoded bytes, sha256={}]",
                        encoded.len(),
                        sha256(encoded.as_bytes())
                    ));
                }
            }
        }
        _ => {}
    }
}

fn tool_event_denied(outcome: &PersistedEventOutcome) -> bool {
    outcome.blocked || outcome.failed || outcome.permission_decision.as_deref() == Some("deny")
}

fn apply_updated_input(
    call: &mut LocalAgentToolCall,
    outcome: &PersistedEventOutcome,
    tool_definitions: &[Value],
) -> Result<(), String> {
    let Some(updated) = outcome.updated_input.as_ref() else {
        return Ok(());
    };
    if tool_event_denied(outcome) {
        return Ok(());
    }
    if !updated.is_object() {
        return Err(format!(
            "trusted {} Hook updatedInput for {} must be a JSON object",
            outcome.event_type, call.tool_name
        ));
    }
    validate_tool_input(&call.tool_name, updated, tool_definitions).map_err(|error| {
        format!(
            "trusted {} Hook updatedInput for {} is invalid: {error}",
            outcome.event_type, call.tool_name
        )
    })?;
    call.arguments = updated.clone();
    call.validate()
}

fn validate_tool_input(
    tool_name: &str,
    input: &Value,
    tool_definitions: &[Value],
) -> Result<(), String> {
    let mut matching = tool_definitions
        .iter()
        .filter(|definition| tool_definition_name(definition) == Some(tool_name));
    let definition = matching
        .next()
        .ok_or_else(|| "the tool was not present in this model request".to_string())?;
    if matching.next().is_some() {
        return Err("the model request contained duplicate tool definitions".to_string());
    }
    let schema = definition
        .pointer("/parameters")
        .or_else(|| definition.pointer("/function/parameters"))
        .ok_or_else(|| "the model-visible tool definition has no parameters schema".to_string())?;
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| format!("the model-visible tool schema is invalid: {error}"))?;
    if let Some(error) = validator.iter_errors(input).next() {
        return Err(format!("schema validation failed: {error}"));
    }
    Ok(())
}

fn tool_definition_name(definition: &Value) -> Option<&str> {
    definition
        .pointer("/name")
        .and_then(Value::as_str)
        .or_else(|| definition.pointer("/function/name").and_then(Value::as_str))
}

fn hook_tool_denial(event_type: &str, reason: &str) -> LocalAgentToolOutcome {
    LocalAgentToolOutcome::Failed {
        error: format!("{event_type} Hook denied this tool call: {reason}"),
        detail: json!({
            "phase": "trusted_plugin_hook",
            "hook_event_name": event_type,
            "denied": true,
        }),
    }
}

fn required_input_string<'a>(run: &'a LocalAgentRunRecord, field: &str) -> Result<&'a str, String> {
    run.input
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{} Run is missing {field}", run.run_id))
}

fn matching_hooks(
    bundles: Vec<LocalPluginHookBundleRecord>,
    event: &HookEvent,
) -> Result<Vec<(LocalPluginHookBundleRecord, LocalPluginHookDefinition)>, String> {
    let mut matches = Vec::new();
    for bundle in bundles {
        if !bundle.spec.enabled {
            continue;
        }
        for hook in &bundle.spec.hooks {
            if hook.event != event.event_type {
                continue;
            }
            let matched = match hook.matcher.as_deref() {
                Some(pattern) => Regex::new(pattern)
                    .map_err(|error| format!("trusted Hook matcher is invalid: {error}"))?
                    .is_match(&event.matcher_value),
                None => true,
            };
            if matched {
                if matches.len() >= MAXIMUM_MATCHING_HOOKS_PER_EVENT {
                    return Err(format!(
                        "Hook event matches more than {MAXIMUM_MATCHING_HOOKS_PER_EVENT} definitions"
                    ));
                }
                matches.push((bundle.clone(), hook.clone()));
            }
        }
    }
    Ok(matches)
}

fn completion_from_result(
    event_type: &str,
    hook: &LocalPluginHookDefinition,
    result: &LocalHookInvocationResult,
) -> LocalHookInvocationCompletion {
    let stdout_sha256 = Some(sha256(&result.stdout));
    let stderr_sha256 = (!result.stderr.is_empty()).then(|| sha256(&result.stderr));
    let mut completion = LocalHookInvocationCompletion {
        status: "failed".to_string(),
        exit_code: result.exit_code,
        timed_out: result.timed_out,
        duration_ms: result.duration_ms,
        stdout_sha256,
        stdout_bytes: u64::try_from(result.stdout.len()).unwrap_or(u64::MAX),
        stdout_overflow: result.stdout_overflow,
        stderr_sha256,
        stderr_bytes: u64::try_from(result.stderr.len()).unwrap_or(u64::MAX),
        stderr_overflow: result.stderr_overflow,
        stderr_summary: hook_diagnostic_summary(result),
        additional_context: None,
        stdout_spill: (result.stdout.len() > OUTPUT_SPILL_THRESHOLD_BYTES)
            .then(|| LocalHookOutputSpill::new(result.stdout.clone())),
        stderr_spill: (result.stderr.len() > OUTPUT_SPILL_THRESHOLD_BYTES)
            .then(|| LocalHookOutputSpill::new(result.stderr.clone())),
    };
    if result.timed_out
        || result.stdout_overflow
        || result.stderr_overflow
        || result.exit_code != Some(0)
    {
        return completion;
    }
    if result.stdout.iter().all(u8::is_ascii_whitespace) {
        completion.status = "succeeded".to_string();
        return completion;
    }
    if hook.is_async {
        completion.status = "succeeded".to_string();
        let Some(value) = parse_hook_json(&result.stdout, true) else {
            completion.stderr_summary =
                Some("async Hook stdout contained no protocol JSON".to_string());
            return completion;
        };
        let mut persisted = PersistedInvocationOutput {
            system_message: value
                .get("systemMessage")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            ..PersistedInvocationOutput::default()
        };
        if let Some(specific) = value.get("hookSpecificOutput").and_then(Value::as_object) {
            if specific.get("hookEventName").and_then(Value::as_str) == Some(event_type) {
                persisted.additional_context = specific
                    .get("additionalContext")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }
        }
        let total_context_bytes = persisted
            .additional_context
            .as_ref()
            .map_or(0, String::len)
            .saturating_add(persisted.system_message.as_ref().map_or(0, String::len));
        if total_context_bytes
            > usize::try_from(hook.additional_context_limit).unwrap_or(usize::MAX)
        {
            completion.status = "failed".to_string();
            completion.stderr_summary = Some("async Hook context exceeds its limit".to_string());
            return completion;
        }
        if persisted != PersistedInvocationOutput::default() {
            completion.additional_context = serde_json::to_string(&persisted).ok();
            if completion.additional_context.is_none() {
                completion.status = "failed".to_string();
            }
        }
        return completion;
    }
    let Some(output) = parse_hook_output(&result.stdout) else {
        completion.stderr_summary = Some("Hook stdout was not valid protocol JSON".to_string());
        return completion;
    };
    if !output.r#continue {
        completion.status = "blocked".to_string();
        completion.stderr_summary = output
            .stop_reason
            .as_deref()
            .map(|reason| safe_summary(reason.as_bytes()))
            .or_else(|| Some("Hook requested continue=false".to_string()));
    } else {
        completion.status = "succeeded".to_string();
    }
    let mut persisted = PersistedInvocationOutput::default();
    if let Some(decision) = output.decision {
        if !matches!(decision.behavior.as_str(), "allow" | "deny" | "ask") {
            completion.status = "failed".to_string();
            return completion;
        }
        persisted.permission_decision = Some(decision.behavior);
        persisted.decision_reason = decision.message;
        persisted.updated_input = decision.updated_input;
    }
    if let Some(specific) = output.hook_specific_output {
        if specific.hook_event_name != event_type {
            completion.status = "failed".to_string();
            completion.additional_context = None;
            return completion;
        }
        if let Some(context) = specific.additional_context {
            if context.len() > usize::try_from(hook.additional_context_limit).unwrap_or(usize::MAX)
            {
                completion.status = "failed".to_string();
                completion.additional_context = None;
                return completion;
            }
            persisted.additional_context = Some(context);
        }
        if let Some(decision) = specific.permission_decision {
            if !matches!(decision.as_str(), "allow" | "deny" | "ask") {
                completion.status = "failed".to_string();
                return completion;
            }
            persisted.permission_decision = Some(decision);
        }
        if specific.permission_decision_reason.is_some() {
            persisted.decision_reason = specific.permission_decision_reason;
        }
        if specific.updated_input.is_some() {
            persisted.updated_input = specific.updated_input;
        }
    }
    if persisted != PersistedInvocationOutput::default() {
        completion.additional_context = serde_json::to_string(&persisted).ok();
        if completion.additional_context.is_none() {
            completion.status = "failed".to_string();
        }
    }
    completion
}

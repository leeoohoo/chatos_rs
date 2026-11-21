use super::super::planning::{
    StoryAgentMemoryCheckpoint, StoryAgentMemoryScope, StoryAgentMessage, StoryAgentMessageRole,
    StoryAgentToolCall, StoryPlanningRun,
};
use super::{memory_contract_error, synced_digest, MEMORY_SOURCE_ID};
use chrono::{DateTime, SecondsFormat, Utc};
use memory_engine_sdk::{EngineRecord, UpsertRecordInput};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub(super) fn validate_checkpoint(
    run: &StoryPlanningRun,
    owner_user_id: &str,
    memory: &StoryAgentMemoryCheckpoint,
) -> Result<(), String> {
    validate_scope(&memory.scope, owner_user_id)?;
    if memory.scope.run_id != run.checkpoint.id
        || memory.scope.runtime_scope != run.checkpoint.scope
        || memory.scope.thread_id
            != format!(
                "client-agent:story:{}:{}",
                run.project_id, run.checkpoint.id
            )
        || memory.scope.subject_id != format!("agent:story-studio:{}", run.project_id)
        || memory.pinned_message_count == 0
        || memory.pinned_message_count > run.checkpoint.messages.len()
        || memory.synced_message_count > run.checkpoint.messages.len()
        || memory.sync_in_flight_end.is_some_and(|end| {
            end <= memory.synced_message_count
                || end > run.checkpoint.messages.len()
                || end - memory.synced_message_count > 32
        })
        || run.checkpoint.messages[..memory.pinned_message_count]
            .iter()
            .any(|message| {
                !matches!(
                    message.role,
                    StoryAgentMessageRole::System | StoryAgentMessageRole::User
                ) || !message.tool_calls.is_empty()
                    || message.tool_call_id.is_some()
            })
    {
        return Err(memory_contract_error());
    }
    if memory.synced_message_count == 0 {
        if !memory.synced_digest.is_empty() {
            return Err(memory_contract_error());
        }
    } else if synced_digest(run, memory.synced_message_count)? != memory.synced_digest {
        return Err(memory_contract_error());
    }
    Ok(())
}

pub(super) fn validate_scope(
    scope: &StoryAgentMemoryScope,
    owner_identity: &str,
) -> Result<(), String> {
    let owner_hash = if owner_identity.len() == 64
        && owner_identity
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        owner_identity.to_ascii_lowercase()
    } else {
        let owner = owner_identity.trim();
        if owner.is_empty() || owner.len() > 1_024 {
            return Err(memory_contract_error());
        }
        format!("{:x}", Sha256::digest(owner.as_bytes()))
    };
    if scope.tenant_hash != owner_hash
        || scope.source_id != MEMORY_SOURCE_ID
        || scope.project_id.trim().is_empty()
        || scope.project_id.chars().any(char::is_control)
        || scope.thread_id != format!("client-agent:story:{}:{}", scope.project_id, scope.run_id)
        || scope.subject_id != format!("agent:story-studio:{}", scope.project_id)
        || scope.thread_id.trim().is_empty()
        || scope.subject_id.trim().is_empty()
        || scope.run_id.trim().is_empty()
        || scope.runtime_scope.trim().is_empty()
    {
        return Err(memory_contract_error());
    }
    Ok(())
}

pub(super) fn memory_record(
    scope: &StoryAgentMemoryScope,
    record_epoch_unix_ms: u64,
    index: usize,
    messages: &[StoryAgentMessage],
) -> Result<UpsertRecordInput, String> {
    let message = messages.get(index).ok_or_else(memory_contract_error)?;
    let mut payload = Map::new();
    if !message.tool_calls.is_empty() {
        payload.insert(
            "tool_calls".into(),
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {"name": call.name, "arguments": call.arguments}
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some(call_id) = &message.tool_call_id {
        payload.insert("tool_call_id".into(), Value::String(call_id.clone()));
    }
    let mut metadata = Map::from_iter([
        (
            "client_agent_run_id".into(),
            Value::String(scope.run_id.clone()),
        ),
        (
            "client_agent_message_index".into(),
            Value::from(index as u64),
        ),
        (
            "conversation_turn_id".into(),
            Value::String(conversation_turn_id(scope, messages, index)?),
        ),
    ]);
    if let Some(call_id) = &message.tool_call_id {
        metadata.insert("tool_call_id".into(), Value::String(call_id.clone()));
        let (assistant_index, assistant) = messages[..index]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, candidate)| {
                candidate.role == StoryAgentMessageRole::Assistant
                    && candidate.tool_calls.iter().any(|call| &call.id == call_id)
            })
            .ok_or_else(memory_contract_error)?;
        let ordinal = assistant
            .tool_calls
            .iter()
            .position(|call| &call.id == call_id)
            .ok_or_else(memory_contract_error)?;
        metadata.insert(
            "assistant_record_id".into(),
            Value::String(format!(
                "client-agent:{}:message:{assistant_index}",
                scope.run_id
            )),
        );
        metadata.insert(
            "tool_exchange_id".into(),
            Value::String(format!("story:{}:tool:{assistant_index}", scope.run_id)),
        );
        metadata.insert("tool_result_ordinal".into(), Value::from(ordinal as u64));
        metadata.insert(
            "tool_result_count".into(),
            Value::from(assistant.tool_calls.len() as u64),
        );
    }
    if let Some(output) = &message.response_output_json {
        metadata.insert(
            "responses_output".into(),
            serde_json::from_slice(output).map_err(|_| memory_contract_error())?,
        );
    }
    if let Some(usage) = &message.usage {
        metadata.insert(
            "provider_usage".into(),
            json!({
                "input_tokens": usage.input_tokens,
                "cached_tokens": usage.cached_tokens,
                "output_tokens": usage.output_tokens,
                "requests": usage.requests,
            }),
        );
    }
    let index_ms = u64::try_from(index).map_err(|_| memory_contract_error())?;
    let timestamp = record_epoch_unix_ms
        .checked_add(index_ms)
        .and_then(|value| i64::try_from(value).ok())
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .ok_or_else(memory_contract_error)?;
    Ok(UpsertRecordInput {
        id: format!("client-agent:{}:message:{index}", scope.run_id),
        external_record_id: None,
        role: match message.role {
            StoryAgentMessageRole::System => "system",
            StoryAgentMessageRole::User => "user",
            StoryAgentMessageRole::Assistant => "assistant",
            StoryAgentMessageRole::Tool => "tool",
        }
        .into(),
        record_type: "message".into(),
        content: message.content.clone(),
        structured_payload: (!payload.is_empty()).then_some(Value::Object(payload)),
        metadata: Some(Value::Object(metadata)),
        summary_status: None,
        summary_id: None,
        summarized_at: None,
        created_at: timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
    })
}

pub(super) fn enrich_source_unit_records(
    records: &mut [UpsertRecordInput],
    runtime_identity_revision: &str,
) -> Result<(), String> {
    if runtime_identity_revision.trim().is_empty() {
        return Err(memory_contract_error());
    }
    for record in records {
        let digest_input = json!({
            "id": record.id,
            "role": record.role,
            "record_type": record.record_type,
            "content": record.content,
            "structured_payload": record.structured_payload,
        });
        let digest = serde_json::to_vec(&digest_input).map_err(|_| memory_contract_error())?;
        let metadata = record
            .metadata
            .get_or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(memory_contract_error)?;
        metadata.insert(
            "project_context_assertion_id".into(),
            Value::String(runtime_identity_revision.to_string()),
        );
        metadata.insert(
            "content_identity_digest".into(),
            Value::String(format!("sha256:{:x}", Sha256::digest(digest))),
        );
    }
    Ok(())
}

pub(crate) fn current_memory_query(run: &StoryPlanningRun) -> Result<(String, String), String> {
    let memory = run
        .checkpoint
        .memory
        .as_ref()
        .ok_or_else(memory_contract_error)?;
    let (index, message) = run
        .checkpoint
        .messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, message)| {
            message.role == StoryAgentMessageRole::User && !message.content.trim().is_empty()
        })
        .ok_or_else(memory_contract_error)?;
    Ok((
        message.content.clone(),
        conversation_turn_id(&memory.scope, run.checkpoint.messages.as_slice(), index)?,
    ))
}

fn conversation_turn_id(
    scope: &StoryAgentMemoryScope,
    messages: &[StoryAgentMessage],
    index: usize,
) -> Result<String, String> {
    if index >= messages.len() {
        return Err(memory_contract_error());
    }
    let turn_start = messages[..=index]
        .iter()
        .rposition(|message| message.role == StoryAgentMessageRole::User)
        .unwrap_or(index);
    Ok(format!("story:{}:turn:{turn_start}", scope.run_id))
}

pub(super) fn record_matches(
    stored: &EngineRecord,
    expected: &UpsertRecordInput,
    owner_user_id: &str,
    scope: &StoryAgentMemoryScope,
) -> bool {
    stored.tenant_id == owner_user_id
        && stored.source_id == scope.source_id
        && record_matches_hashed_tenant(stored, expected, scope)
}

pub(super) fn record_matches_hashed_tenant(
    stored: &EngineRecord,
    expected: &UpsertRecordInput,
    scope: &StoryAgentMemoryScope,
) -> bool {
    format!("{:x}", Sha256::digest(stored.tenant_id.trim().as_bytes())) == scope.tenant_hash
        && stored.id == expected.id
        && stored.thread_id == scope.thread_id
        && stored.source_id == scope.source_id
        && stored.role == expected.role
        && stored.record_type == expected.record_type
        && stored.content == expected.content
        && stored.structured_payload == expected.structured_payload
        && stored.metadata == expected.metadata
        && stored.created_at == expected.created_at
}

pub(super) fn record_index(record_id: &str, run_id: &str) -> Result<usize, String> {
    let prefix = format!("client-agent:{run_id}:message:");
    let value = record_id
        .strip_prefix(&prefix)
        .ok_or_else(memory_contract_error)?;
    let index = value
        .parse::<usize>()
        .map_err(|_| memory_contract_error())?;
    if index.to_string() != value {
        return Err(memory_contract_error());
    }
    Ok(index)
}

pub(super) fn paired_messages(messages: Vec<StoryAgentMessage>) -> Vec<StoryAgentMessage> {
    let mut remaining_outputs = HashMap::<String, usize>::new();
    for message in &messages {
        if message.role == StoryAgentMessageRole::Tool {
            if let Some(call_id) = &message.tool_call_id {
                *remaining_outputs.entry(call_id.clone()).or_default() += 1;
            }
        }
    }
    let mut seen_calls = HashSet::new();
    let mut retained = Vec::new();
    for mut message in messages {
        match message.role {
            StoryAgentMessageRole::Tool => {
                if let Some(call_id) = message.tool_call_id.as_ref() {
                    if seen_calls.contains(call_id) {
                        retained.push(message.clone());
                    }
                    if let Some(count) = remaining_outputs.get_mut(call_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            remaining_outputs.remove(call_id);
                        }
                    }
                }
            }
            StoryAgentMessageRole::Assistant => {
                let calls = message
                    .tool_calls
                    .iter()
                    .filter(|call| remaining_outputs.get(&call.id).copied().unwrap_or_default() > 0)
                    .cloned()
                    .collect::<Vec<StoryAgentToolCall>>();
                seen_calls.extend(calls.iter().map(|call| call.id.clone()));
                if !message.content.trim().is_empty() || !calls.is_empty() {
                    message.tool_calls = calls;
                    message.response_output_json = None;
                    retained.push(message);
                }
            }
            StoryAgentMessageRole::System | StoryAgentMessageRole::User => {
                if !message.content.trim().is_empty() {
                    retained.push(message);
                }
            }
        }
    }
    retained
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LocalAgentToolStatus,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const TOOL_RESULT_READ_TOOL: &str = "tool_result_read";
pub const TOOL_RESULT_TOOL_NAMES: [&str; 1] = [TOOL_RESULT_READ_TOOL];
pub const TOOL_RESULT_READ_ONLY_TOOLS: [&str; 1] = [TOOL_RESULT_READ_TOOL];

const DEFAULT_PAGE_CHARS: usize = 6_000;
const MAX_PAGE_CHARS: usize = 8_000;
const MAX_MODEL_OUTPUT_CHARS: usize = 12_000;

#[derive(Clone)]
pub struct LocalToolResultReader {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
}

impl LocalToolResultReader {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("tool result reader owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
        })
    }

    pub fn register_into(&self, registry: &mut LocalToolRegistry) -> Result<(), String> {
        let executor: Arc<dyn LocalToolExecutor> = Arc::new(self.clone());
        registry.register_shared(TOOL_RESULT_READ_TOOL, executor)
    }

    async fn execute(&self, invocation: &LocalAgentToolInvocationRecord) -> Result<Value, String> {
        let arguments: ReadArgs = serde_json::from_value(invocation.arguments.clone())
            .map_err(|error| format!("invalid {TOOL_RESULT_READ_TOOL} input: {error}"))?;
        match (
            arguments.source_call_id.as_deref(),
            arguments.source_run_id.as_deref(),
        ) {
            (Some(source_call_id), None) => {
                let source_call_id = validated_reference(source_call_id, "source_call_id")?;
                let source = self
                    .runtime
                    .get_completed_tool_invocation_for_run_call_for_host_worker(
                        &self.owner_user_id,
                        &invocation.run_id,
                        source_call_id,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        "completed source tool result was not found in the current Run".to_string()
                    })?;
                if source.call_id == invocation.call_id {
                    return Err("tool_result_read cannot read its own in-flight result".to_string());
                }
                let content = source_model_text(&source)?;
                page_result(
                    "source_call_id",
                    source_call_id,
                    source.tool_name.as_str(),
                    content.as_str(),
                    arguments.offset.unwrap_or(0),
                    arguments.max_chars.unwrap_or(DEFAULT_PAGE_CHARS),
                )
            }
            (None, Some(source_run_id)) => {
                let source_run_id = validated_reference(source_run_id, "source_run_id")?;
                let current = self
                    .runtime
                    .get_run_for_host_worker(&invocation.run_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "current Run was not found".to_string())?;
                if current.owner_user_id != self.owner_user_id
                    || !run_is_resolved_prerequisite(&current.input, source_run_id)
                {
                    return Err(
                        "source_run_id is not a resolved prerequisite of the current Task"
                            .to_string(),
                    );
                }
                let source = self
                    .runtime
                    .get_run_for_host_worker(source_run_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "prerequisite source Run was not found".to_string())?;
                if source.owner_user_id != self.owner_user_id || !source.status.is_terminal() {
                    return Err("prerequisite source Run is not complete".to_string());
                }
                let content = source_run_model_text(&source)?;
                page_result(
                    "source_run_id",
                    source_run_id,
                    "task_terminal_outcome",
                    content.as_str(),
                    arguments.offset.unwrap_or(0),
                    arguments.max_chars.unwrap_or(DEFAULT_PAGE_CHARS),
                )
            }
            _ => Err("provide exactly one of source_call_id or source_run_id".to_string()),
        }
    }
}

#[async_trait]
impl LocalToolExecutor for LocalToolResultReader {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        Ok(match self.execute(invocation).await {
            Ok(output) => LocalAgentToolOutcome::Succeeded { output },
            Err(error) => LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": "tool_result_read"}),
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    #[serde(default)]
    source_call_id: Option<String>,
    #[serde(default)]
    source_run_id: Option<String>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    max_chars: Option<usize>,
}

fn validated_reference<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return Err(format!("{field} must be 1..=256 characters"));
    }
    Ok(value)
}

fn run_is_resolved_prerequisite(input: &Value, source_run_id: &str) -> bool {
    input
        .get("resolved_prerequisites")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("run_id").and_then(Value::as_str) == Some(source_run_id))
        })
}

fn source_model_text(source: &LocalAgentToolInvocationRecord) -> Result<String, String> {
    let value = source
        .result
        .as_ref()
        .ok_or_else(|| "source tool result has no persisted payload".to_string())?;
    let result = match source.status {
        LocalAgentToolStatus::Succeeded => {
            chatos_mcp_runtime::execution::external_tool_result_from_value(
                source.call_id.clone(),
                source.tool_name.clone(),
                None,
                value,
                Some(usize::MAX),
            )
        }
        LocalAgentToolStatus::Failed | LocalAgentToolStatus::NeedsReview => {
            chatos_mcp_runtime::execution::external_tool_failure_from_value(
                source.call_id.clone(),
                source.tool_name.clone(),
                None,
                Some(value),
                source.error.as_deref().unwrap_or_default(),
                Some(usize::MAX),
            )
        }
        LocalAgentToolStatus::Pending | LocalAgentToolStatus::Running => {
            return Err("source tool result is not complete".to_string())
        }
    };
    Ok(result.content)
}

fn source_run_model_text(
    source: &chatos_local_agent_protocol::LocalAgentRunRecord,
) -> Result<String, String> {
    let outcome = source
        .terminal_outcome
        .as_ref()
        .ok_or_else(|| "prerequisite source Run has no terminal outcome".to_string())?;
    for key in ["content", "result_summary", "answer", "text", "report"] {
        if let Some(text) = outcome.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return Ok(text.to_string());
            }
        }
    }
    if let Some(text) = outcome
        .pointer("/task_acceptance/final_response")
        .and_then(Value::as_str)
    {
        if !text.trim().is_empty() {
            return Ok(text.to_string());
        }
    }
    Ok(outcome.to_string())
}

fn page_result(
    source_reference_field: &str,
    source_reference_id: &str,
    source_tool_name: &str,
    content: &str,
    offset: usize,
    requested_max_chars: usize,
) -> Result<Value, String> {
    let characters: Vec<char> = content.chars().collect();
    let total_chars = characters.len();
    if offset > total_chars {
        return Err(format!(
            "offset {offset} exceeds tool result length {total_chars}"
        ));
    }
    let requested_max_chars = requested_max_chars.clamp(1, MAX_PAGE_CHARS);
    let sha256 = format!("{:x}", Sha256::digest(content.as_bytes()));
    let mut page_chars = requested_max_chars.min(total_chars.saturating_sub(offset));
    loop {
        let next_offset = offset + page_chars;
        let page: String = characters[offset..next_offset].iter().collect();
        let mut output = json!({
            "type": "tool_result_page",
            "source_tool_name": source_tool_name,
            "content": page,
            "offset": offset,
            "returned_chars": page_chars,
            "next_offset": next_offset,
            "total_chars": total_chars,
            "sha256": sha256,
            "complete": next_offset == total_chars
        });
        output[source_reference_field] = Value::String(source_reference_id.to_string());
        if output.to_string().chars().count() <= MAX_MODEL_OUTPUT_CHARS || page_chars == 0 {
            return Ok(output);
        }
        page_chars = (page_chars / 2).max(1);
    }
}

pub fn tool_result_model_tools() -> Vec<Value> {
    vec![json!({
        "type": "function",
        "name": TOOL_RESULT_READ_TOOL,
        "description": "Read an exact page from a complete durable tool result. Use source_call_id for a result in the current Run, or source_run_id only when a prerequisite descriptor supplies it. Start at offset 0, continue with next_offset until complete=true, concatenate content in order, and verify sha256. This tool never reads another owner, unrelated Run, or unrelated Task.",
        "parameters": {
            "type": "object",
            "properties": {
                "source_call_id": {"type": "string", "minLength": 1, "maxLength": 256},
                "source_run_id": {"type": "string", "minLength": 1, "maxLength": 256},
                "offset": {"type": "integer", "minimum": 0, "default": 0},
                "max_chars": {"type": "integer", "minimum": 1, "maximum": MAX_PAGE_CHARS, "default": DEFAULT_PAGE_CHARS}
            },
            "oneOf": [
                {"required": ["source_call_id"]},
                {"required": ["source_run_id"]}
            ],
            "additionalProperties": false
        }
    })]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nineteen_thousand_character_result_reconstructs_exactly_with_checksum() {
        let source = format!("{}{}", "解".repeat(9_980), "A".repeat(9_981));
        assert_eq!(source.chars().count(), 19_961);
        let expected_sha = format!("{:x}", Sha256::digest(source.as_bytes()));
        let mut offset = 0usize;
        let mut reconstructed = String::new();
        loop {
            let page = page_result(
                "source_call_id",
                "call-solution",
                "solution_get_workspace",
                &source,
                offset,
                8_000,
            )
            .expect("page");
            assert_eq!(page["sha256"], expected_sha);
            assert_eq!(page["offset"], offset);
            let chunk = page["content"].as_str().expect("content");
            reconstructed.push_str(chunk);
            offset = page["next_offset"].as_u64().expect("next offset") as usize;
            assert!(page.to_string().chars().count() <= MAX_MODEL_OUTPUT_CHARS);
            if page["complete"].as_bool() == Some(true) {
                break;
            }
        }
        assert_eq!(reconstructed, source);
        assert_eq!(
            format!("{:x}", Sha256::digest(reconstructed.as_bytes())),
            expected_sha
        );
    }

    #[test]
    fn heavily_escaped_content_is_reduced_without_losing_offsets() {
        let source = "\u{0}".repeat(8_000);
        let first =
            page_result("source_call_id", "call", "tool", &source, 0, 8_000).expect("first page");
        assert!(first["returned_chars"].as_u64().unwrap() < 8_000);
        assert!(first.to_string().chars().count() <= MAX_MODEL_OUTPUT_CHARS);
        assert_eq!(first["next_offset"], first["returned_chars"]);
    }
}

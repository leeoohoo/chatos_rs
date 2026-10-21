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
pub const TOOL_RESULT_QUERY_TOOL: &str = "tool_result_query";
pub const TOOL_RESULT_TOOL_NAMES: [&str; 2] = [TOOL_RESULT_READ_TOOL, TOOL_RESULT_QUERY_TOOL];
pub const TOOL_RESULT_READ_ONLY_TOOLS: [&str; 2] = [TOOL_RESULT_READ_TOOL, TOOL_RESULT_QUERY_TOOL];

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
        registry.register_shared(TOOL_RESULT_READ_TOOL, executor.clone())?;
        registry.register_shared(TOOL_RESULT_QUERY_TOOL, executor)
    }

    async fn execute(&self, invocation: &LocalAgentToolInvocationRecord) -> Result<Value, String> {
        let arguments: ReadArgs = serde_json::from_value(invocation.arguments.clone())
            .map_err(|error| format!("invalid {TOOL_RESULT_READ_TOOL} input: {error}"))?;
        let source = self
            .load_source(
                invocation,
                arguments.source.source_call_id.as_deref(),
                arguments.source.source_run_id.as_deref(),
            )
            .await?;
        page_result(
            source.reference_field,
            &source.reference_id,
            &source.tool_name,
            &source.content,
            arguments.offset.unwrap_or(0),
            arguments.max_chars.unwrap_or(DEFAULT_PAGE_CHARS),
        )
    }

    async fn query(&self, invocation: &LocalAgentToolInvocationRecord) -> Result<Value, String> {
        let arguments: QueryArgs = serde_json::from_value(invocation.arguments.clone())
            .map_err(|error| format!("invalid {TOOL_RESULT_QUERY_TOOL} input: {error}"))?;
        let source = self
            .load_source(
                invocation,
                arguments.source.source_call_id.as_deref(),
                arguments.source.source_run_id.as_deref(),
            )
            .await?;
        query_result(&source, arguments)
    }

    async fn load_source(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
        source_call_id: Option<&str>,
        source_run_id: Option<&str>,
    ) -> Result<SourceResult, String> {
        match (source_call_id, source_run_id) {
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
                Ok(SourceResult {
                    reference_field: "source_call_id",
                    reference_id: source_call_id.to_string(),
                    tool_name: source.tool_name,
                    content,
                })
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
                Ok(SourceResult {
                    reference_field: "source_run_id",
                    reference_id: source_run_id.to_string(),
                    tool_name: "task_terminal_outcome".to_string(),
                    content,
                })
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
        let result = match invocation.tool_name.as_str() {
            TOOL_RESULT_READ_TOOL => self.execute(invocation).await,
            TOOL_RESULT_QUERY_TOOL => self.query(invocation).await,
            _ => Err("unsupported tool result operation".to_string()),
        };
        Ok(match result {
            Ok(output) => LocalAgentToolOutcome::Succeeded { output },
            Err(error) => LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": invocation.tool_name}),
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    #[serde(flatten)]
    source: SourceArgs,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    max_chars: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceArgs {
    #[serde(default)]
    source_call_id: Option<String>,
    #[serde(default)]
    source_run_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArgs {
    #[serde(flatten)]
    source: SourceArgs,
    mode: QueryMode,
    #[serde(default)]
    json_pointer: Option<String>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    max_chars: Option<usize>,
    #[serde(default)]
    context_chars: Option<usize>,
    #[serde(default)]
    match_limit: Option<usize>,
    #[serde(default)]
    case_sensitive: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum QueryMode {
    JsonPointer,
    Search,
    Head,
    Tail,
    Range,
}

struct SourceResult {
    reference_field: &'static str,
    reference_id: String,
    tool_name: String,
    content: String,
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
    Ok(chatos_mcp_runtime::execution::external_tool_result_reference_content(&result))
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

fn query_result(source: &SourceResult, arguments: QueryArgs) -> Result<Value, String> {
    let total_chars = source.content.chars().count();
    let sha256 = format!("{:x}", Sha256::digest(source.content.as_bytes()));
    let mut base = json!({
        "type": "tool_result_query",
        "source_tool_name": source.tool_name,
        "total_chars": total_chars,
        "sha256": sha256,
    });
    base[source.reference_field] = Value::String(source.reference_id.clone());
    match arguments.mode {
        QueryMode::JsonPointer => {
            let pointer = arguments
                .json_pointer
                .as_deref()
                .ok_or_else(|| "json_pointer mode requires json_pointer".to_string())?;
            if pointer.len() > 2_048 || (!pointer.is_empty() && !pointer.starts_with('/')) {
                return Err("json_pointer must be an RFC 6901 pointer up to 2048 bytes".to_string());
            }
            let document: Value = serde_json::from_str(&source.content)
                .map_err(|_| "source result is not a JSON document".to_string())?;
            let selected = document
                .pointer(pointer)
                .ok_or_else(|| "json_pointer did not match the source result".to_string())?;
            let selected_text = selected
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| selected.to_string());
            bounded_selection(
                base,
                "json_pointer",
                selected_text.as_str(),
                arguments.offset.unwrap_or(0),
                arguments.max_chars.unwrap_or(DEFAULT_PAGE_CHARS),
                Some(json!({"json_pointer": pointer})),
            )
        }
        QueryMode::Search => search_result(base, &source.content, arguments),
        QueryMode::Head => bounded_selection(
            base,
            "head",
            &source.content,
            0,
            arguments.max_chars.unwrap_or(DEFAULT_PAGE_CHARS),
            None,
        ),
        QueryMode::Tail => {
            let count = arguments
                .max_chars
                .unwrap_or(DEFAULT_PAGE_CHARS)
                .clamp(1, MAX_PAGE_CHARS);
            bounded_selection(
                base,
                "tail",
                &source.content,
                total_chars.saturating_sub(count),
                count,
                None,
            )
        }
        QueryMode::Range => bounded_selection(
            base,
            "range",
            &source.content,
            arguments.offset.unwrap_or(0),
            arguments.max_chars.unwrap_or(DEFAULT_PAGE_CHARS),
            None,
        ),
    }
}

fn bounded_selection(
    mut base: Value,
    mode: &str,
    content: &str,
    offset: usize,
    requested_max_chars: usize,
    detail: Option<Value>,
) -> Result<Value, String> {
    let characters: Vec<char> = content.chars().collect();
    if offset > characters.len() {
        return Err(format!(
            "offset {offset} exceeds selected result length {}",
            characters.len()
        ));
    }
    let mut page_chars = requested_max_chars
        .clamp(1, MAX_PAGE_CHARS)
        .min(characters.len().saturating_sub(offset));
    loop {
        let next = offset + page_chars;
        base["mode"] = Value::String(mode.to_string());
        base["content"] = Value::String(characters[offset..next].iter().collect());
        base["selection_offset"] = json!(offset);
        base["returned_chars"] = json!(page_chars);
        base["selection_total_chars"] = json!(characters.len());
        base["next_selection_offset"] = if next < characters.len() {
            json!(next)
        } else {
            Value::Null
        };
        base["complete"] = json!(next == characters.len());
        if let Some(detail) = &detail {
            base["selection"] = detail.clone();
        }
        if base.to_string().chars().count() <= MAX_MODEL_OUTPUT_CHARS || page_chars == 0 {
            return Ok(base);
        }
        page_chars = (page_chars / 2).max(1);
    }
}

fn search_result(mut base: Value, content: &str, arguments: QueryArgs) -> Result<Value, String> {
    let query = arguments
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "search mode requires a non-empty search string".to_string())?;
    if query.chars().count() > 512 {
        return Err("search cannot exceed 512 characters".to_string());
    }
    let characters: Vec<char> = content.chars().collect();
    let start = arguments.offset.unwrap_or(0);
    if start > characters.len() {
        return Err(format!(
            "offset {start} exceeds tool result length {}",
            characters.len()
        ));
    }
    let context = arguments.context_chars.unwrap_or(400).min(2_000);
    let limit = arguments.match_limit.unwrap_or(5).clamp(1, 20);
    let searchable: String = characters[start..].iter().collect();
    let case_sensitive = arguments.case_sensitive.unwrap_or(false);
    let haystack = if case_sensitive {
        searchable.clone()
    } else {
        searchable.to_ascii_lowercase()
    };
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_ascii_lowercase()
    };
    let mut byte_cursor = 0usize;
    let mut matches = Vec::new();
    let mut has_more = false;
    while let Some(relative_byte) = haystack[byte_cursor..].find(&needle) {
        let byte_offset = byte_cursor + relative_byte;
        let match_start = start + searchable[..byte_offset].chars().count();
        let match_end = match_start + query.chars().count();
        if matches.len() == limit {
            has_more = true;
            break;
        }
        let snippet_start = match_start.saturating_sub(context);
        let snippet_end = (match_end + context).min(characters.len());
        matches.push(json!({
            "match_offset": match_start,
            "match_end": match_end,
            "content_offset": snippet_start,
            "content": characters[snippet_start..snippet_end].iter().collect::<String>(),
        }));
        byte_cursor = byte_offset + needle.len().max(1);
    }
    base["mode"] = json!("search");
    base["query"] = json!(query);
    base["case_sensitive"] = json!(case_sensitive);
    base["matches"] = Value::Array(matches);
    base["match_count"] = json!(base["matches"].as_array().map_or(0, Vec::len));
    base["next_offset"] = if has_more {
        base["matches"]
            .as_array()
            .and_then(|items| items.last())
            .and_then(|item| item.get("match_end"))
            .cloned()
            .unwrap_or(json!(start))
    } else {
        Value::Null
    };
    while base.to_string().chars().count() > MAX_MODEL_OUTPUT_CHARS {
        let Some((count, next_offset)) = base["matches"].as_array_mut().and_then(|items| {
            if items.len() > 1 {
                items.pop();
            } else {
                let item = items.first_mut()?;
                let text = item.get("content")?.as_str()?.chars().collect::<Vec<_>>();
                if text.len() <= 1 {
                    return None;
                }
                let content_offset = item.get("content_offset")?.as_u64()? as usize;
                let match_offset = item.get("match_offset")?.as_u64()? as usize;
                let match_end = item.get("match_end")?.as_u64()? as usize;
                let match_length = match_end.saturating_sub(match_offset);
                let keep = (text.len() / 2).max(match_length).max(1);
                let local_match = match_offset.saturating_sub(content_offset);
                let before = local_match.min(keep.saturating_sub(match_length) / 2);
                let slice_start = local_match.saturating_sub(before);
                let slice_end = (slice_start + keep).min(text.len());
                item["content"] = Value::String(text[slice_start..slice_end].iter().collect());
                item["content_offset"] = json!(content_offset + slice_start);
            }
            Some((
                items.len(),
                items
                    .last()
                    .and_then(|item| item.get("match_end"))
                    .cloned()
                    .unwrap_or(json!(start)),
            ))
        }) else {
            break;
        };
        base["match_count"] = json!(count);
        base["next_offset"] = next_offset;
    }
    Ok(base)
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
    vec![
        json!({
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
        }),
        json!({
            "type": "function",
            "name": TOOL_RESULT_QUERY_TOOL,
            "description": "Extract useful content from a complete durable tool result without reading every page. Use mode=json_pointer for structured JSON, search for matching snippets, head/tail for boundaries, or range for a known character offset. Results retain the source total_chars and sha256. Use source_call_id in the current Run or a supplied prerequisite source_run_id; unrelated Runs and owners are rejected.",
            "parameters": {
                "type": "object",
                "properties": {
                    "source_call_id": {"type": "string", "minLength": 1, "maxLength": 256},
                    "source_run_id": {"type": "string", "minLength": 1, "maxLength": 256},
                    "mode": {"type": "string", "enum": ["json_pointer", "search", "head", "tail", "range"]},
                    "json_pointer": {"type": "string", "maxLength": 2048},
                    "search": {"type": "string", "minLength": 1, "maxLength": 512},
                    "offset": {"type": "integer", "minimum": 0, "default": 0},
                    "max_chars": {"type": "integer", "minimum": 1, "maximum": MAX_PAGE_CHARS, "default": DEFAULT_PAGE_CHARS},
                    "context_chars": {"type": "integer", "minimum": 0, "maximum": 2000, "default": 400},
                    "match_limit": {"type": "integer", "minimum": 1, "maximum": 20, "default": 5},
                    "case_sensitive": {"type": "boolean", "default": false}
                },
                "required": ["mode"],
                "oneOf": [
                    {"required": ["source_call_id"]},
                    {"required": ["source_run_id"]}
                ],
                "additionalProperties": false
            }
        }),
    ]
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

    #[test]
    fn hundred_kilobyte_result_finds_useful_content_in_one_search() {
        let content = format!(
            "{}\nDECISION: bind frontend to service_url returned by the allocator\n{}",
            "background ".repeat(7_000),
            "appendix ".repeat(7_000)
        );
        assert!(content.len() > 100_000);
        let source = SourceResult {
            reference_field: "source_call_id",
            reference_id: "call-large".into(),
            tool_name: "browser_snapshot".into(),
            content: content.clone(),
        };
        let result = query_result(
            &source,
            QueryArgs {
                source: SourceArgs {
                    source_call_id: Some("call-large".into()),
                    source_run_id: None,
                },
                mode: QueryMode::Search,
                json_pointer: None,
                search: Some("DECISION:".into()),
                offset: None,
                max_chars: None,
                context_chars: Some(120),
                match_limit: Some(3),
                case_sensitive: Some(true),
            },
        )
        .expect("query");
        assert_eq!(result["match_count"], 1);
        assert!(result["matches"][0]["content"]
            .as_str()
            .unwrap()
            .contains("service_url"));
        assert_eq!(
            result["sha256"],
            format!("{:x}", Sha256::digest(content.as_bytes()))
        );
        assert!(result.to_string().chars().count() <= MAX_MODEL_OUTPUT_CHARS);
    }

    #[test]
    fn json_pointer_extracts_one_branch_without_reading_the_document() {
        let content = json!({
            "summary": "short",
            "items": (0..2_000).map(|index| json!({"id": index, "value": format!("value-{index}")})).collect::<Vec<_>>()
        })
        .to_string();
        let source = SourceResult {
            reference_field: "source_call_id",
            reference_id: "call-json".into(),
            tool_name: "solution_get_workspace".into(),
            content,
        };
        let result = query_result(
            &source,
            QueryArgs {
                source: SourceArgs {
                    source_call_id: Some("call-json".into()),
                    source_run_id: None,
                },
                mode: QueryMode::JsonPointer,
                json_pointer: Some("/items/142/value".into()),
                search: None,
                offset: None,
                max_chars: None,
                context_chars: None,
                match_limit: None,
                case_sensitive: None,
            },
        )
        .expect("query");
        assert_eq!(result["content"], "value-142");
        assert_eq!(result["complete"], true);
        assert_eq!(result["selection"]["json_pointer"], "/items/142/value");
    }
}

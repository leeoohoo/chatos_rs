// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp_runtime::ToolResult;
use serde_json::{json, Value};
use uuid::Uuid;

/// Builds the read-only recovery calls required after a Code Maintainer write
/// reports stale file state. The runtime never guesses replacement content:
/// it only refreshes the exact paths identified by the structured conflict so
/// the next model step can rebuild the edit from current project contents.
pub fn automatic_file_write_recovery_calls(
    tool_results: &[ToolResult],
    available_tools: &[Value],
) -> Result<Vec<Value>, String> {
    let available_names = available_tools
        .iter()
        .filter_map(tool_definition_name)
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let mut calls = Vec::new();

    for result in tool_results {
        if result.success || !is_code_maintainer_write_tool(result.name.as_str()) {
            continue;
        }
        let recovery_specs = structured_file_write_recoveries(result.content.as_str());
        if recovery_specs.is_empty() {
            continue;
        }
        for (path, required, args) in recovery_specs {
            let tool_name = matching_recovery_tool_name(
                result.name.as_str(),
                required.as_str(),
                available_names.as_slice(),
            )
            .ok_or_else(|| {
                format!(
                    "MCP capability invariant violated: {} returned a recovery-capable write failure for {}, but the required {} capability is not available",
                    result.name, path, required
                )
            })?;
            let dedupe_key = format!("{tool_name}\n{}", args);
            if !seen.insert(dedupe_key) {
                continue;
            }
            calls.push(json!({
                "id": format!("runtime_recovery_{}", Uuid::new_v4()),
                "type": "function",
                "function": {
                    "name": tool_name,
                    "arguments": args.to_string(),
                }
            }));
        }
    }

    Ok(calls)
}

fn is_code_maintainer_write_tool(name: &str) -> bool {
    name.contains("code_maintainer")
        && ["stage_edit_batch", "commit_edit_session"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

fn structured_file_write_recoveries(content: &str) -> Vec<(String, String, Value)> {
    content
        .match_indices('{')
        .filter_map(|(index, _)| {
            serde_json::Deserializer::from_str(&content[index..])
                .into_iter::<Value>()
                .next()
                .and_then(Result::ok)
        })
        .find(|payload| {
            matches!(
                payload.get("category").and_then(Value::as_str),
                Some("stale_context" | "expected_match")
            )
        })
        .map(recovery_specs_from_payload)
        .unwrap_or_default()
}

fn recovery_specs_from_payload(payload: Value) -> Vec<(String, String, Value)> {
    let mut specs = Vec::new();
    if let Some(path) = payload.get("path").and_then(Value::as_str) {
        if let Some(spec) = recovery_spec_from_value(path, payload.get("recovery")) {
            specs.push(spec);
        }
    }
    if let Some(conflicts) = payload.get("conflicts").and_then(Value::as_array) {
        for conflict in conflicts {
            let Some(path) = conflict.get("path").and_then(Value::as_str) else {
                continue;
            };
            if let Some(spec) = recovery_spec_from_value(path, conflict.get("recovery")) {
                specs.push(spec);
            }
        }
    }
    specs
}

fn recovery_spec_from_value(
    path: &str,
    recovery: Option<&Value>,
) -> Option<(String, String, Value)> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let recovery = recovery?;
    let required = recovery
        .get("required_next_tool")
        .and_then(Value::as_str)
        .unwrap_or("read_file_raw");
    let args = recovery
        .get("recommended_args")
        .cloned()
        .unwrap_or_else(|| default_recovery_args(path, required));
    Some((path.to_string(), required.to_string(), args))
}

fn default_recovery_args(path: &str, required: &str) -> Value {
    match required {
        "list_dir" => json!({ "path": "." }),
        _ => json!({ "path": path }),
    }
}

fn tool_definition_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(Value::as_str)
        .or_else(|| tool.pointer("/function/name").and_then(Value::as_str))
}

fn matching_recovery_tool_name<'a>(
    failed_tool_name: &str,
    required_tool: &str,
    available_names: &'a [&str],
) -> Option<&'a str> {
    let namespace = failed_tool_name
        .split_once("_write_")
        .map(|(prefix, _)| prefix);
    let preferred = namespace.map(|prefix| format!("{prefix}_read_{required_tool}"));
    preferred
        .as_deref()
        .and_then(|name| available_names.iter().copied().find(|item| *item == name))
        .or_else(|| {
            available_names
                .iter()
                .copied()
                .find(|name| *name == required_tool)
        })
        .or_else(|| {
            let namespace = namespace?;
            available_names
                .iter()
                .copied()
                .find(|name| name.starts_with(namespace) && name.ends_with(required_tool))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_result(success: bool, content: &str) -> ToolResult {
        ToolResult {
            tool_call_id: "call-1".to_string(),
            name: "code_maintainer_write_stage_edit_batch".to_string(),
            success,
            is_error: !success,
            is_stream: false,
            conversation_turn_id: None,
            content: content.to_string(),
            result: None,
            fatal_error: false,
            transient_model_input: None,
        }
    }

    #[test]
    fn stale_write_builds_only_the_scoped_read_recovery() {
        let stale = tool_result(
            false,
            r#"tool failed: {"category":"stale_context","path":"README.md","recovery":{"required_next_tool":"read_file_raw","recommended_args":{"path":"README.md"}}}"#,
        );
        let calls = automatic_file_write_recovery_calls(
            &[stale],
            &[json!({"name": "code_maintainer_read_read_file_raw"})],
        )
        .expect("automatic recovery calls");

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0]["function"]["name"],
            "code_maintainer_read_read_file_raw"
        );
        assert_eq!(
            serde_json::from_str::<Value>(calls[0]["function"]["arguments"].as_str().unwrap())
                .unwrap(),
            json!({"path": "README.md"})
        );
    }

    #[test]
    fn recovery_deduplicates_the_same_conflict_path() {
        let stale = tool_result(
            false,
            r#"{"category":"stale_context","conflicts":[{"path":"src/lib.rs","recovery":{"required_next_tool":"read_file_raw","recommended_args":{"path":"src/lib.rs"}}},{"path":"src/lib.rs","recovery":{"required_next_tool":"read_file_raw","recommended_args":{"path":"src/lib.rs"}}}]}"#,
        );
        let calls = automatic_file_write_recovery_calls(
            &[stale],
            &[json!({"name": "code_maintainer_read_read_file_raw"})],
        )
        .expect("deduplicated automatic recovery calls");

        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn missing_read_capability_is_a_program_invariant_error() {
        let stale = tool_result(
            false,
            r#"{"category":"expected_match","path":"src/lib.rs","recovery":{"required_next_tool":"read_file_raw"}}"#,
        );
        let error = automatic_file_write_recovery_calls(&[stale], &[])
            .expect_err("missing recovery capability must fail");

        assert!(error.contains("MCP capability invariant violated"));
    }
}

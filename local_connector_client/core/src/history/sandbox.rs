// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::format::compact_json;

#[derive(Debug)]
pub(crate) struct SandboxToolCallDetails {
    pub(crate) tool_name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) display: String,
}

#[derive(Default)]
pub(super) struct SandboxToolResultPreview {
    pub(super) exit_code: Option<i32>,
    pub(super) timed_out: Option<bool>,
    pub(super) stdout: Option<String>,
    pub(super) stderr: Option<String>,
    pub(super) error: Option<String>,
}

pub(crate) fn sandbox_tool_call_details(body: &Value) -> Option<SandboxToolCallDetails> {
    let (tool_name, arguments) = if body.get("method").and_then(Value::as_str) == Some("tools/call")
    {
        let params = body.get("params")?;
        (
            params
                .get("name")
                .and_then(Value::as_str)?
                .trim()
                .to_string(),
            params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({})),
        )
    } else {
        let tool_name = body
            .get("tool")
            .or_else(|| body.get("name"))
            .and_then(Value::as_str)?
            .trim()
            .to_string();
        (
            tool_name,
            body.get("arguments").cloned().unwrap_or_else(|| json!({})),
        )
    };
    if tool_name.is_empty() {
        return None;
    }
    let command =
        sandbox_command_from_arguments(&arguments).unwrap_or_else(|| format!("mcp:{tool_name}"));
    let cwd = arguments
        .get("cwd")
        .or_else(|| arguments.get("path"))
        .or_else(|| arguments.get("working_dir"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let display = if command == format!("mcp:{tool_name}") {
        format!("mcp:{tool_name} {}", compact_json(&arguments, 480))
    } else {
        command.clone()
    };
    Some(SandboxToolCallDetails {
        tool_name,
        command,
        args: Vec::new(),
        cwd,
        display,
    })
}

pub(super) fn extract_sandbox_tool_result(body: &Value) -> SandboxToolResultPreview {
    let result_body = body
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .or_else(|| body.get("result").cloned())
        .unwrap_or_else(|| body.clone());
    let mut preview = SandboxToolResultPreview {
        exit_code: result_body
            .get("exit_code")
            .or_else(|| result_body.get("code"))
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        timed_out: result_body
            .get("timed_out")
            .or_else(|| result_body.get("timeout"))
            .and_then(Value::as_bool),
        stdout: result_body
            .get("stdout")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        stderr: result_body
            .get("stderr")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        error: extract_error_message(body).or_else(|| extract_error_message(&result_body)),
    };
    if preview.stdout.is_none() {
        preview.stdout = result_body
            .get("output")
            .or_else(|| result_body.get("text"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    preview
}

fn sandbox_command_from_arguments(arguments: &Value) -> Option<String> {
    ["command", "common", "cmd", "shell_command", "script"]
        .iter()
        .find_map(|key| {
            arguments
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

fn extract_error_message(value: &Value) -> Option<String> {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return value
            .get("error")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| Some("sandbox tool call failed".to_string()));
    }
    value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .map(ToOwned::to_owned)
}

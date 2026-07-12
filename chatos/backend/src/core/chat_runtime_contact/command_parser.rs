// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::models::chatos_agent_types::{
    ChatosAgentRuntimeCommandSummaryDto, ChatosAgentRuntimeContextDto,
};

use super::types::{
    ParsedContactCommandInvocation, ParsedImplicitCommandSelection,
    CONTACT_COMMAND_READER_TOOL_NAME,
};

pub fn parse_contact_command_invocation(
    user_message: &str,
    runtime_context: Option<&ChatosAgentRuntimeContextDto>,
) -> Option<ParsedContactCommandInvocation> {
    let trimmed = user_message.trim();
    let command_line = trimmed.strip_prefix('/')?;
    let command_line = command_line.trim();
    if command_line.is_empty() {
        return None;
    }

    let mut parts = command_line.splitn(2, char::is_whitespace);
    let command_token = parts.next().unwrap_or_default().trim();
    if command_token.is_empty() {
        return None;
    }
    let command_arguments = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let runtime_context = runtime_context?;
    if runtime_context.runtime_commands.is_empty() {
        return None;
    }
    let expected = normalize_lookup_token(command_token);
    let command = runtime_context
        .runtime_commands
        .iter()
        .find(|item| command_aliases(item).iter().any(|alias| alias == &expected))?;

    Some(ParsedContactCommandInvocation {
        command_ref: command.command_ref.trim().to_string(),
        name: command.name.trim().to_string(),
        plugin_source: command.plugin_source.trim().to_string(),
        source_path: command.source_path.trim().to_string(),
        description: command
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        argument_hint: command
            .argument_hint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        content: command.content.trim().to_string(),
        arguments: command_arguments,
    })
}

pub fn parse_implicit_command_selections_from_tools_end(
    payload: &Value,
) -> Vec<ParsedImplicitCommandSelection> {
    let mut out = Vec::new();
    let Some(tool_results) = payload.get("tool_results").and_then(Value::as_array) else {
        return out;
    };

    for tool_result in tool_results {
        let Some(name) = tool_result.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.trim() != CONTACT_COMMAND_READER_TOOL_NAME {
            continue;
        }
        if tool_result
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        if !tool_result
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        let Some(content) = tool_result.get("content").and_then(Value::as_str) else {
            continue;
        };
        let Ok(content_value) = serde_json::from_str::<Value>(content) else {
            continue;
        };
        let plugin_source = content_value
            .get("plugin_source")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let source_path = content_value
            .get("source_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let (Some(plugin_source), Some(source_path)) = (plugin_source, source_path) else {
            continue;
        };

        out.push(ParsedImplicitCommandSelection {
            command_ref: content_value
                .get("command_ref")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            name: content_value
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            plugin_source,
            source_path,
        });
    }

    out
}

fn normalize_lookup_token(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn command_aliases(command: &ChatosAgentRuntimeCommandSummaryDto) -> Vec<String> {
    let mut out = Vec::new();
    let command_ref = command.command_ref.trim();
    if !command_ref.is_empty() {
        out.push(command_ref.to_ascii_lowercase());
    }
    let command_name = command.name.trim();
    if !command_name.is_empty() {
        out.push(command_name.to_ascii_lowercase());
    }

    let normalized_source_path = command.source_path.trim().replace('\\', "/");
    if !normalized_source_path.is_empty() {
        let file_name = normalized_source_path
            .rsplit('/')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(normalized_source_path.as_str());
        let file_name = file_name
            .strip_suffix(".md")
            .unwrap_or(file_name)
            .trim()
            .to_ascii_lowercase();
        if !file_name.is_empty() {
            out.push(file_name);
        }
    }
    out.sort();
    out.dedup();
    out
}

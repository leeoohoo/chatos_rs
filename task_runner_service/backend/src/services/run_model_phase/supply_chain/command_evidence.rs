// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::{NodeVulnerabilityCounts, TerminalCommandResult, TerminalWaitResult};

struct TerminalOutput {
    exit_code: Option<i64>,
    output: String,
    output_truncated: bool,
}

pub(super) fn terminal_result(payload: &Value) -> Option<TerminalCommandResult> {
    let content = terminal_content(payload);
    let result = payload
        .get("result")
        .map(chatos_mcp_runtime::structured_result_payload)
        .filter(|value| value.is_object());
    let command = content
        .as_ref()
        .and_then(|value| value.get("common").or_else(|| value.get("command")))
        .and_then(Value::as_str)
        .or_else(|| {
            result
                .and_then(|value| value.get("common").or_else(|| value.get("command")))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|command| !command.is_empty())?;
    let terminal_output = terminal_output(result, content.as_ref());
    Some(TerminalCommandResult {
        command: command.to_string(),
        exit_code: terminal_output.exit_code,
        output: terminal_output.output,
        output_truncated: terminal_output.output_truncated,
    })
}

pub(super) fn terminal_wait_result(payload: &Value) -> Option<TerminalWaitResult> {
    let content = terminal_content(payload);
    let result = payload
        .get("result")
        .map(chatos_mcp_runtime::structured_result_payload)
        .filter(|value| value.is_object());
    let process_id = result
        .and_then(|value| value.get("process_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            content
                .as_ref()
                .and_then(|value| value.get("process_id"))
                .and_then(Value::as_str)
        })?
        .trim();
    if process_id.is_empty() {
        return None;
    }
    let terminal_output = terminal_output(result, content.as_ref());
    Some(TerminalWaitResult {
        process_id: process_id.to_string(),
        exit_code: terminal_output.exit_code,
        output: terminal_output.output,
        output_truncated: terminal_output.output_truncated,
    })
}

fn terminal_output(result: Option<&Value>, content: Option<&Value>) -> TerminalOutput {
    let exit_code = result
        .and_then(|value| value.get("exit_code"))
        .and_then(Value::as_i64)
        .or_else(|| {
            content
                .and_then(|value| value.get("exit_code"))
                .and_then(Value::as_i64)
        });
    let output = result
        .and_then(|value| value.get("output"))
        .and_then(Value::as_str)
        .or_else(|| {
            content
                .and_then(|value| value.get("output"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string();
    let output_truncated = result
        .and_then(|value| value.get("truncated"))
        .and_then(Value::as_bool)
        .or_else(|| {
            content
                .and_then(|value| value.get("truncated"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    TerminalOutput {
        exit_code,
        output,
        output_truncated,
    }
}

pub(super) fn terminal_process_id(payload: &Value) -> Option<String> {
    let content = terminal_content(payload);
    payload
        .get("result")
        .map(chatos_mcp_runtime::structured_result_payload)
        .and_then(|value| value.get("process_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            content
                .as_ref()
                .and_then(|value| value.get("process_id"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn terminal_content(payload: &Value) -> Option<Value> {
    let parsed = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .filter(Value::is_object)?;
    Some(chatos_mcp_runtime::structured_result_payload(&parsed).clone())
}

pub(super) fn command_uses_registry(command: &str, expected_registry: &str) -> bool {
    let expected_registry = expected_registry.trim().trim_end_matches('/');
    if expected_registry.is_empty() {
        return false;
    }
    let tokens = command
        .split_whitespace()
        .map(|token| token.trim_matches(|character| matches!(character, '\'' | '"')))
        .collect::<Vec<_>>();
    tokens
        .windows(2)
        .any(|pair| pair[0] == "--registry" && pair[1].trim_end_matches('/') == expected_registry)
        || tokens.iter().any(|token| {
            token
                .strip_prefix("--registry=")
                .is_some_and(|registry| registry.trim_end_matches('/') == expected_registry)
        })
}

pub(super) fn node_package_manager(command: &str) -> Option<&'static str> {
    if command_invokes_executable(command, "pnpm") {
        Some("pnpm")
    } else if command_invokes_executable(command, "yarn") {
        Some("yarn")
    } else if command_invokes_executable(command, "bun") {
        Some("bun")
    } else if command_invokes_executable(command, "npm") {
        Some("npm")
    } else {
        None
    }
}

pub(super) fn is_node_install_command(command: &str) -> bool {
    [
        &["npm", "ci"][..],
        &["npm", "install"][..],
        &["pnpm", "install"][..],
        &["yarn", "install"][..],
        &["bun", "install"][..],
    ]
    .iter()
    .any(|invocation| command_invocation_segment(command, invocation).is_some())
        && !command.contains("--package-lock-only")
}

pub(super) fn install_scripts_are_disabled(command: &str, package_manager: Option<&str>) -> bool {
    let install_segment = [
        &["npm", "ci"][..],
        &["npm", "install"][..],
        &["pnpm", "install"][..],
        &["yarn", "install"][..],
        &["bun", "install"][..],
    ]
    .iter()
    .find_map(|invocation| command_invocation_segment(command, invocation))
    .unwrap_or(command);
    match package_manager {
        Some("yarn") => {
            install_segment.contains("--mode=skip-builds")
                || install_segment.contains("--mode skip-builds")
        }
        Some("npm" | "pnpm" | "bun") | None => install_segment.contains("--ignore-scripts"),
        Some(_) => false,
    }
}

pub(super) fn command_masks_failure(command: &str) -> bool {
    let compact = command
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact.contains("||true") || compact.contains(";true")
}

pub(super) fn is_lockfile_command(command: &str) -> bool {
    command_invocation_segment(command, &["npm", "install"])
        .is_some_and(|segment| segment.contains("--package-lock-only"))
        || command_invocation_segment(command, &["pnpm", "install"])
            .is_some_and(|segment| segment.contains("--lockfile-only"))
        || command_invocation_segment(command, &["yarn", "install"])
            .is_some_and(|segment| segment.contains("--mode=update-lockfile"))
}

pub(super) fn approved_rebuild_packages(command: &str) -> Option<Vec<String>> {
    let segment = command_invocation_segment(command, &["npm", "rebuild"])
        .or_else(|| command_invocation_segment(command, &["pnpm", "rebuild"]))?;
    let packages = segment
        .split_whitespace()
        .skip(2)
        .take_while(|value| !value.starts_with('-') && !matches!(*value, "&&" | ";" | "||"))
        .map(|value| value.trim_matches(|character| matches!(character, '\'' | '"')))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    Some(packages)
}

pub(super) fn rebuild_completed_successfully(
    command: &str,
    exit_code: Option<i64>,
    output: &str,
) -> bool {
    if command_masks_failure(command) {
        return false;
    }
    if exit_code == Some(0) {
        return true;
    }

    let segments = shell_command_segments(command).collect::<Vec<_>>();
    let rebuild_index = segments.iter().position(|segment| {
        command_invocation_segment(segment, &["npm", "rebuild"]).is_some()
            || command_invocation_segment(segment, &["pnpm", "rebuild"]).is_some()
    });
    let rebuild_preceded_a_later_command =
        rebuild_index.is_some_and(|index| index + 1 < segments.len());

    rebuild_preceded_a_later_command
        && output.lines().any(|line| {
            line.trim()
                .eq_ignore_ascii_case("rebuilt dependencies successfully")
        })
}

pub(super) fn is_node_audit_command(command: &str) -> bool {
    command_invocation_segment(command, &["npm", "audit"]).is_some()
        || command_invocation_segment(command, &["pnpm", "audit"]).is_some()
        || command_invocation_segment(command, &["yarn", "npm", "audit"]).is_some()
}

pub(super) fn audit_command_matches_level(command: &str, audit_level: &str) -> bool {
    let command = command.to_ascii_lowercase();
    let Some(segment) = command_invocation_segment(&command, &["npm", "audit"])
        .or_else(|| command_invocation_segment(&command, &["pnpm", "audit"]))
        .or_else(|| command_invocation_segment(&command, &["yarn", "npm", "audit"]))
    else {
        return false;
    };
    segment.contains("--json")
        && (segment.contains(format!("--audit-level={audit_level}").as_str())
            || segment.contains(format!("--audit-level {audit_level}").as_str())
            || segment.contains(format!("--severity={audit_level}").as_str())
            || segment.contains(format!("--severity {audit_level}").as_str()))
}

pub(super) fn parse_vulnerability_counts(output: &str) -> Option<NodeVulnerabilityCounts> {
    if let Ok(value) = serde_json::from_str::<Value>(output.trim()) {
        if let Some(counts) = vulnerability_counts_from_value(&value) {
            return Some(counts);
        }
    }
    for (start, character) in output.char_indices() {
        if character != '{' {
            continue;
        }
        let mut values = serde_json::Deserializer::from_str(&output[start..]).into_iter::<Value>();
        let Some(Ok(value)) = values.next() else {
            continue;
        };
        if let Some(counts) = vulnerability_counts_from_value(&value) {
            return Some(counts);
        }
    }
    None
}

pub(super) fn vulnerability_counts_from_value(value: &Value) -> Option<NodeVulnerabilityCounts> {
    let vulnerabilities = value.pointer("/metadata/vulnerabilities")?;
    Some(NodeVulnerabilityCounts {
        total: vulnerabilities.get("total")?.as_u64()?,
        info: vulnerabilities.get("info")?.as_u64()?,
        low: vulnerabilities.get("low")?.as_u64()?,
        moderate: vulnerabilities.get("moderate")?.as_u64()?,
        high: vulnerabilities.get("high")?.as_u64()?,
        critical: vulnerabilities.get("critical")?.as_u64()?,
    })
}

pub(super) fn command_invokes_executable(command: &str, executable: &str) -> bool {
    shell_command_segments(command).any(|segment| {
        command_tokens(segment)
            .first()
            .is_some_and(|token| executable_name(token) == executable)
    })
}

pub(super) fn command_invocation_segment<'a>(
    command: &'a str,
    invocation: &[&str],
) -> Option<&'a str> {
    shell_command_segments(command).find(|segment| {
        let tokens = command_tokens(segment);
        tokens.len() >= invocation.len()
            && tokens
                .iter()
                .zip(invocation)
                .enumerate()
                .all(|(index, (token, expected))| {
                    if index == 0 {
                        executable_name(token) == *expected
                    } else {
                        token.trim_matches(|character| matches!(character, '\'' | '"')) == *expected
                    }
                })
    })
}

pub(super) fn shell_command_segments(command: &str) -> impl Iterator<Item = &str> {
    command
        .split(['\n', ';'])
        .flat_map(|segment| segment.split("&&"))
        .flat_map(|segment| segment.split("||"))
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
}

pub(super) fn command_tokens(segment: &str) -> Vec<&str> {
    let tokens = segment.split_whitespace().collect::<Vec<_>>();
    let mut start = 0;
    while let Some(token) = tokens.get(start) {
        let cleaned = token.trim_matches(|character| matches!(character, '(' | '{'));
        if matches!(cleaned, "then" | "do" | "!" | "env" | "command")
            || (cleaned.contains('=') && !cleaned.starts_with('-'))
        {
            start += 1;
            continue;
        }
        break;
    }
    tokens[start..].to_vec()
}

pub(super) fn executable_name(token: &str) -> &str {
    token
        .trim_matches(|character| matches!(character, '(' | '{' | '\'' | '"'))
        .rsplit('/')
        .next()
        .unwrap_or(token)
}

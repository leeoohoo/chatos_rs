// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn validate_stdio_command(
    index: usize,
    command: &str,
    args: &[String],
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let field = format!("mcpServers[{index}].command");
    if command.trim().is_empty() {
        issue(issues, field.as_str(), "command cannot be empty");
        return;
    }
    if command.contains('/') {
        if let Err(message) = normalize_plugin_relative_path(command) {
            issue(issues, field.as_str(), message);
        }
    } else if !command
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        issue(
            issues,
            field.as_str(),
            "command must be a signed relative path or reviewed command identifier",
        );
    }

    let shell = command
        .rsplit('/')
        .next()
        .unwrap_or(command)
        .to_ascii_lowercase();
    let has_shell_eval = match shell.as_str() {
        "sh" | "bash" | "zsh" => args.iter().any(|arg| arg == "-c"),
        "cmd" | "cmd.exe" => args.iter().any(|arg| arg.eq_ignore_ascii_case("/c")),
        "powershell" | "powershell.exe" | "pwsh" => args
            .iter()
            .any(|arg| matches!(arg.to_ascii_lowercase().as_str(), "-command" | "-c")),
        _ => false,
    };
    if has_shell_eval {
        issue(
            issues,
            field.as_str(),
            "generic shell evaluation is not allowed for plugin MCP entrypoints",
        );
    }
}

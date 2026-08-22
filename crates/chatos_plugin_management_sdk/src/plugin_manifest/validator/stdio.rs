// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn validate_npm_bin(
    index: usize,
    bin: &str,
    args: &[String],
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let field = format!("mcpServers[{index}].bin");
    if bin.trim().is_empty() {
        issue(issues, field.as_str(), "bin cannot be empty");
        return;
    }
    if bin.contains('/')
        || !bin
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        issue(
            issues,
            field.as_str(),
            "bin must be a package.json executable name",
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains('\0') || arg.len() > 8 * 1024)
    {
        issue(
            issues,
            format!("mcpServers[{index}].args").as_str(),
            "MCP arguments must be bounded text without NUL bytes",
        );
    }
}

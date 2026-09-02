// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn validate_npm_bin(
    index: usize,
    bin: &str,
    args: &[String],
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    validate_package_executable(
        format!("mcpServers[{index}]").as_str(),
        "MCP",
        bin,
        args,
        issues,
    );
}

pub(super) fn validate_package_executable(
    field: &str,
    runtime_label: &str,
    bin: &str,
    args: &[String],
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let bin_field = format!("{field}.bin");
    if bin.trim().is_empty() {
        issue(issues, bin_field.as_str(), "bin cannot be empty");
        return;
    }
    if bin.contains('/')
        || !bin
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        issue(
            issues,
            bin_field.as_str(),
            "bin must be a package.json executable name",
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains('\0') || arg.len() > 8 * 1024)
    {
        issue(
            issues,
            format!("{field}.args").as_str(),
            format!("{runtime_label} arguments must be bounded text without NUL bytes"),
        );
    }
}

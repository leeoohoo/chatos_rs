// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_sandbox_contract::{FileSystemAccessMode, RequestPermissionProfile};

#[derive(Debug, Clone)]
pub(crate) struct RiskSummary {
    pub(crate) level: String,
    pub(crate) reason: Option<String>,
}

pub(crate) fn classify_command(command: &str) -> RiskSummary {
    let lower = command.to_ascii_lowercase();
    let high_risk_markers = [
        "sudo ",
        "rm -rf",
        "curl ",
        "wget ",
        "| sh",
        "| bash",
        "chmod -r",
        "chown -r",
        "/etc/",
        "/usr/",
        "/system/",
        ".env",
        "id_rsa",
        "id_ed25519",
        "private_key",
        "kubectl ",
    ];
    if let Some(marker) = high_risk_markers
        .iter()
        .find(|marker| lower.contains(**marker))
    {
        return RiskSummary {
            level: "high".to_string(),
            reason: Some(format!("matched risk marker `{marker}`")),
        };
    }

    RiskSummary {
        level: "low".to_string(),
        reason: None,
    }
}

pub(crate) fn classify_command_request(
    command: &str,
    permissions: Option<&RequestPermissionProfile>,
) -> RiskSummary {
    let command_risk = classify_command(command);
    if command_risk.level == "high" {
        return command_risk;
    }
    let Some(permissions) = permissions else {
        return command_risk;
    };
    if permissions
        .network
        .as_ref()
        .and_then(|network| network.enabled)
        == Some(true)
    {
        return RiskSummary {
            level: "high".to_string(),
            reason: Some("command requests temporary network access".to_string()),
        };
    }
    let entries = permissions
        .file_system
        .as_ref()
        .map(|file_system| file_system.normalized_entries())
        .unwrap_or_default();
    if entries
        .iter()
        .any(|entry| entry.access == FileSystemAccessMode::Write)
    {
        return RiskSummary {
            level: "high".to_string(),
            reason: Some("command requests temporary filesystem write access".to_string()),
        };
    }
    if entries
        .iter()
        .any(|entry| entry.access == FileSystemAccessMode::Read)
    {
        return RiskSummary {
            level: "medium".to_string(),
            reason: Some("command requests temporary filesystem read access".to_string()),
        };
    }
    command_risk
}

/// Returns a local, deterministic approval reason for commands that only inspect
/// the current toolchain. These probes must not depend on a second model call:
/// they are commonly used to decide how a task should be bootstrapped, and a
/// model-backed approval creates a circular dependency when the model route is
/// degraded.
pub(crate) fn static_environment_probe_approval(
    command: &str,
    permissions: Option<&RequestPermissionProfile>,
) -> Option<&'static str> {
    if permissions.is_some() {
        return None;
    }
    let command = command.trim();
    if command.is_empty()
        || command.contains(['|', '>', '<', '`', '$', '\n', '\r'])
        || command.contains("$(")
    {
        return None;
    }
    let segments = command
        .split([';', '&'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty()
        || !segments
            .iter()
            .all(|segment| is_safe_probe_segment(segment))
    {
        return None;
    }
    Some("safe local environment/toolchain inspection")
}

fn is_safe_probe_segment(segment: &str) -> bool {
    let tokens = segment.split_whitespace().collect::<Vec<_>>();
    match tokens.as_slice() {
        ["pwd"] | ["go", "version"] => true,
        [tool, flag]
            if matches!(
                *tool,
                "java"
                    | "javac"
                    | "mvn"
                    | "gradle"
                    | "node"
                    | "npm"
                    | "pnpm"
                    | "yarn"
                    | "python"
                    | "python3"
                    | "git"
                    | "cargo"
                    | "rustc"
                    | "dotnet"
                    | "ruby"
            ) && matches!(*flag, "--version" | "-version" | "-v") =>
        {
            true
        }
        ["command", "-v", tool] | ["which", tool] => is_plain_tool_name(tool),
        _ => false,
    }
}

fn is_plain_tool_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{AdditionalNetworkPermissions, RequestPermissionProfile};

    #[test]
    fn network_permission_request_is_high_risk_even_for_benign_command() {
        let request = RequestPermissionProfile {
            file_system: None,
            network: Some(AdditionalNetworkPermissions {
                enabled: Some(true),
            }),
        };
        let risk = classify_command_request("true", Some(&request));
        assert_eq!(risk.level, "high");
        assert!(risk
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("network"));
    }

    #[test]
    fn statically_approves_bounded_toolchain_probes() {
        assert!(
            static_environment_probe_approval("pwd; java -version && mvn --version", None)
                .is_some()
        );
        assert!(static_environment_probe_approval("node -v && npm --version", None).is_some());
        assert!(static_environment_probe_approval("command -v cargo", None).is_some());
    }

    #[test]
    fn does_not_statically_approve_shell_expansion_or_mutation() {
        for command in [
            "echo $HOME",
            "java -version | tee version.txt",
            "mvn test",
            "curl https://example.com",
            "rm -rf build",
        ] {
            assert!(static_environment_probe_approval(command, None).is_none());
        }
    }
}

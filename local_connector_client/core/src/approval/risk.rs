// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
        "docker ",
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
}
use chatos_sandbox_contract::{FileSystemAccessMode, RequestPermissionProfile};

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

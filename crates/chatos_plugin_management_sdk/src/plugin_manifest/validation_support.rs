// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use reqwest::Url;

use super::PluginManifestValidationIssue;

pub(super) fn validate_stdio_environment(
    index: usize,
    env: &BTreeMap<String, String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if env.len() > 64 {
        issue(
            issues,
            format!("mcpServers[{index}].env").as_str(),
            "stdio MCP environment exceeds 64 variables",
        );
    }
    for (name, value) in env {
        let field = format!("mcpServers[{index}].env.{name}");
        let valid_name = !name.is_empty()
            && name.len() <= 128
            && name
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if !valid_name {
            issue(
                issues,
                field.as_str(),
                "stdio MCP environment variable name is invalid",
            );
            continue;
        }
        if is_host_controlled_environment_name(name) {
            issue(
                issues,
                field.as_str(),
                "stdio MCP environment variable is controlled by the Host",
            );
        }
        let secret_name = value
            .strip_prefix("${credential:")
            .and_then(|value| value.strip_suffix('}'));
        let valid_secret = secret_name.is_some_and(|secret_name| {
            !secret_name.is_empty()
                && secret_name.len() <= 128
                && secret_name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        });
        if !valid_secret {
            issue(
                issues,
                field.as_str(),
                "stdio MCP environment values must be exact ${credential:<secret_name>} templates",
            );
        }
    }
}

fn is_host_controlled_environment_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    matches!(
        name.as_str(),
        "PATH"
            | "HOME"
            | "SHELL"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "COMSPEC"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "USERPROFILE"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "PROMPT_COMMAND"
    ) || name.starts_with("LD_")
        || name.starts_with("DYLD_")
        || name.starts_with("XDG_")
}

pub(super) fn validate_brand_color(
    value: Option<&str>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let Some(value) = value else {
        return;
    };
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid {
        issue(
            issues,
            "interface.brandColor",
            "brand color must use #RRGGBB",
        );
    }
}

pub(super) fn validate_optional_email(
    value: Option<&str>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if let Some(value) = value {
        let valid = value
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
        if !valid {
            issue(issues, "author.email", "email address is invalid");
        }
    }
}

pub(super) fn validate_optional_https_url(
    field: &str,
    value: Option<&str>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if let Some(value) = value {
        validate_https_url(field.to_string(), value, issues);
    }
}

fn validate_https_url(field: String, value: &str, issues: &mut Vec<PluginManifestValidationIssue>) {
    let valid = Url::parse(value)
        .ok()
        .is_some_and(|url| url.scheme() == "https" && url.host_str().is_some());
    if !valid {
        issue(
            issues,
            field.as_str(),
            "URL must be an absolute https:// URL",
        );
    }
}

pub(super) fn validate_mcp_http_url(
    field: String,
    value: &str,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let valid = Url::parse(value).ok().is_some_and(|url| {
        let Some(host) = url.host_str() else {
            return false;
        };
        let loopback = host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .ok()
                .is_some_and(|address| address.is_loopback());
        url.scheme() == "https" || (url.scheme() == "http" && loopback)
    });
    if !valid {
        issue(
            issues,
            field.as_str(),
            "MCP URL must use https://, except for http:// loopback development servers",
        );
    }
}

pub(super) fn required_text(
    issues: &mut Vec<PluginManifestValidationIssue>,
    field: &str,
    value: &str,
) {
    if value.trim().is_empty() {
        issue(issues, field, "field is required");
    }
}

pub(super) fn issue(
    issues: &mut Vec<PluginManifestValidationIssue>,
    field: &str,
    message: impl Into<String>,
) {
    issues.push(PluginManifestValidationIssue {
        field: field.to_string(),
        message: message.into(),
    });
}

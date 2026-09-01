// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use crate::models::normalized_optional;

pub(super) fn normalize_git_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = normalized_optional(value) else {
        return Ok(None);
    };
    if value.len() > 2048 {
        return Err("git_url 过长".to_string());
    }
    if value.chars().any(char::is_whitespace) {
        return Err("git_url 不能包含空白字符".to_string());
    }
    let lower = value.to_ascii_lowercase();
    let is_supported = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("ssh://")
        || lower.starts_with("git@");
    if !is_supported {
        return Err(
            "git_url 需要是常见 Git 地址，例如 https://、ssh:// 或 git@host:path".to_string(),
        );
    }
    if lower.starts_with("https://") || lower.starts_with("http://") {
        let url = reqwest::Url::parse(value.as_str())
            .map_err(|err| format!("git_url 不是有效 URL: {err}"))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(
                "git_url 不能包含用户名、密码或访问令牌，请使用本机 Git 凭据管理".to_string(),
            );
        }
    }
    Ok(Some(value))
}

pub(super) fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter_map(|value| normalized_optional(Some(value)))
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

pub(super) fn task_runner_status_is_active(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "ready" | "queued" | "running" | "processing" | "in_progress"
    )
}

#[cfg(test)]
mod tests {
    use super::normalize_git_url;

    #[test]
    fn git_url_rejects_embedded_http_credentials() {
        assert!(normalize_git_url(Some(
            "https://user:token@example.com/owner/repo.git".to_string()
        ))
        .is_err());
        assert!(
            normalize_git_url(Some("https://token@example.com/owner/repo.git".to_string()))
                .is_err()
        );
        assert_eq!(
            normalize_git_url(Some("https://example.com/owner/repo.git".to_string())).unwrap(),
            Some("https://example.com/owner/repo.git".to_string())
        );
        assert_eq!(
            normalize_git_url(Some("git@example.com:owner/repo.git".to_string())).unwrap(),
            Some("git@example.com:owner/repo.git".to_string())
        );
    }
}

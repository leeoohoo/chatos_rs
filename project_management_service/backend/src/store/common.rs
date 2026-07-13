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

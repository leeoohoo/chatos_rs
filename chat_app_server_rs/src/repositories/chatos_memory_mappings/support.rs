// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::project::PUBLIC_PROJECT_ID;

pub(super) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn normalize_project_id(value: &str) -> String {
    match normalize_optional_text(Some(value)).as_deref() {
        Some("0") | None => PUBLIC_PROJECT_ID.to_string(),
        Some(value) => value.to_string(),
    }
}

pub fn default_project_name(project_id: &str) -> String {
    if matches!(project_id.trim(), "0" | PUBLIC_PROJECT_ID) {
        "未指定项目".to_string()
    } else {
        format!("项目 {}", project_id.trim())
    }
}

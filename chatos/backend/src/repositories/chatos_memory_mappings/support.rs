// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn normalize_project_id(value: &str) -> String {
    normalize_optional_text(Some(value)).unwrap_or_default()
}

pub fn default_project_name(project_id: &str) -> String {
    format!("项目 {}", project_id.trim())
}

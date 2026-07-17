// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::project::PUBLIC_PROJECT_ID;

pub(super) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn is_concrete_project_id(project_id: &str) -> bool {
    let normalized = project_id.trim();
    !normalized.is_empty() && normalized != "0" && normalized != PUBLIC_PROJECT_ID
}

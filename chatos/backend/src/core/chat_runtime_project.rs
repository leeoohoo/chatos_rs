// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::chat_runtime::normalize_project_id;

#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedProjectRuntime {
    pub(crate) project_id: Option<String>,
    pub(crate) project_name: Option<String>,
    pub(crate) project_root: Option<String>,
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub(crate) async fn resolve_project_runtime_context(
    _user_id: Option<&str>,
    project_id: Option<String>,
    project_root: Option<String>,
) -> ResolvedProjectRuntime {
    ResolvedProjectRuntime {
        project_id: normalize_project_id(project_id.as_deref()),
        project_name: None,
        project_root: normalize_optional_string(project_root),
    }
}

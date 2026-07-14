// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::project::{normalize_project_id, ProjectService, PUBLIC_PROJECT_ID};

#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedProjectRuntime {
    pub(crate) project_id: Option<String>,
    pub(crate) project_name: Option<String>,
    pub(crate) project_root: Option<String>,
    pub(crate) source_type: Option<String>,
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

fn normalize_path_text(raw: &str) -> String {
    let mut out = raw.trim().replace('\\', "/");
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

pub(crate) async fn resolve_project_runtime_context(
    user_id: Option<&str>,
    project_id: Option<String>,
    project_root: Option<String>,
) -> ResolvedProjectRuntime {
    let mut resolved_project_id = normalize_optional_string(project_id);
    let mut resolved_project_root = normalize_optional_string(project_root);

    let Some(project_id) = resolved_project_id.clone() else {
        return ResolvedProjectRuntime {
            project_id: resolved_project_id,
            project_root: resolved_project_root,
            ..ResolvedProjectRuntime::default()
        };
    };
    let project_id = normalize_project_id(project_id.as_str());
    if project_id == PUBLIC_PROJECT_ID {
        resolved_project_id = Some(PUBLIC_PROJECT_ID.to_string());
        return ResolvedProjectRuntime {
            project_id: resolved_project_id,
            project_root: resolved_project_root,
            ..ResolvedProjectRuntime::default()
        };
    }
    resolved_project_id = Some(project_id.clone());

    let project = match ProjectService::get_by_id(project_id.as_str()).await {
        Ok(Some(project)) => project,
        _ => {
            resolved_project_id = None;
            return ResolvedProjectRuntime {
                project_id: resolved_project_id,
                project_root: resolved_project_root,
                ..ResolvedProjectRuntime::default()
            };
        }
    };

    if let (Some(uid), Some(project_owner)) = (user_id, project.user_id.as_deref()) {
        if project_owner != uid {
            resolved_project_id = None;
            return ResolvedProjectRuntime {
                project_id: resolved_project_id,
                project_root: resolved_project_root,
                ..ResolvedProjectRuntime::default()
            };
        }
    }

    let expected_root = normalize_path_text(project.root_path.as_str());
    match resolved_project_root.clone() {
        Some(current_root) => {
            if normalize_path_text(current_root.as_str()) != expected_root {
                resolved_project_root = Some(project.root_path);
            }
        }
        None => {
            resolved_project_root = Some(project.root_path);
        }
    }

    ResolvedProjectRuntime {
        project_id: resolved_project_id,
        project_name: normalize_optional_string(Some(project.name)),
        project_root: resolved_project_root,
        source_type: normalize_optional_string(project.source_type),
    }
}

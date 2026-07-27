// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::warn;

use super::support::normalize_optional_text;
use crate::api::fs::policy::FsPathPolicy;
use crate::core::auth::AuthUser;
use crate::models::project::{harness_project_id_from_root_path, HARNESS_PROJECT_ROOT_PREFIX};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ResolvedRuntimeProjectRoot {
    pub(super) logical_root: Option<String>,
    pub(super) local_workspace_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeProjectRootKind {
    Harness(String),
    InvalidHarness,
    LocalCandidate(String),
}

fn classify_runtime_project_root(raw: &str) -> RuntimeProjectRootKind {
    let raw = raw.trim().to_string();
    if harness_project_id_from_root_path(raw.as_str()).is_some() {
        RuntimeProjectRootKind::Harness(raw)
    } else if raw.starts_with(HARNESS_PROJECT_ROOT_PREFIX) {
        RuntimeProjectRootKind::InvalidHarness
    } else {
        RuntimeProjectRootKind::LocalCandidate(raw)
    }
}

pub(super) async fn resolve_runtime_project_root(
    user_id: Option<&str>,
    raw: Option<String>,
) -> ResolvedRuntimeProjectRoot {
    let Some(raw) = normalize_optional_text(raw.as_deref()) else {
        return ResolvedRuntimeProjectRoot::default();
    };
    match classify_runtime_project_root(raw.as_str()) {
        RuntimeProjectRootKind::Harness(logical_root) => ResolvedRuntimeProjectRoot {
            logical_root: Some(logical_root),
            local_workspace_root: None,
        },
        RuntimeProjectRootKind::InvalidHarness => {
            warn!(
                project_root = raw.as_str(),
                "invalid Harness project root dropped"
            );
            ResolvedRuntimeProjectRoot::default()
        }
        RuntimeProjectRootKind::LocalCandidate(local_candidate) => {
            let local_workspace_root =
                authorize_runtime_workspace_dir(user_id, Some(local_candidate)).await;
            ResolvedRuntimeProjectRoot {
                logical_root: local_workspace_root.clone(),
                local_workspace_root,
            }
        }
    }
}

pub(super) async fn authorize_runtime_workspace_dir(
    user_id: Option<&str>,
    raw: Option<String>,
) -> Option<String> {
    let raw = normalize_optional_text(raw.as_deref())?;
    if raw.starts_with(HARNESS_PROJECT_ROOT_PREFIX) {
        warn!(
            workspace_dir = raw.as_str(),
            "cloud logical project root is not a local workspace directory"
        );
        return None;
    }
    let Some(user_id) = user_id.map(str::trim).filter(|value| !value.is_empty()) else {
        warn!("runtime workspace path dropped: missing effective user id");
        return None;
    };
    let auth = AuthUser {
        user_id: user_id.to_string(),
        role: "user".to_string(),
    };
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(policy) => policy,
        Err(err) => {
            warn!(
                user_id,
                error = err.message(),
                "runtime workspace path dropped: policy unavailable"
            );
            return None;
        }
    };
    let authorized = match policy.authorize_existing_dir(
        raw.as_str(),
        "运行工作目录不存在或不是目录",
        "运行工作目录不存在或不是目录",
    ) {
        Ok(path) => path,
        Err(err) => {
            warn!(
                user_id,
                workspace_dir = raw.as_str(),
                error = err.message(),
                "runtime workspace path dropped: unauthorized"
            );
            return None;
        }
    };
    if let Err(err) = policy.require_write(&authorized) {
        warn!(
            user_id,
            workspace_dir = raw.as_str(),
            error = err.message(),
            "runtime workspace path dropped: not writable"
        );
        return None;
    }
    Some(authorized.path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preserves_harness_root_as_logical_context_only() {
        let resolved = resolve_runtime_project_root(
            Some("user-1"),
            Some("harness://project/project-1".to_string()),
        )
        .await;

        assert_eq!(
            resolved.logical_root.as_deref(),
            Some("harness://project/project-1")
        );
        assert_eq!(resolved.local_workspace_root, None);
    }

    #[tokio::test]
    async fn harness_root_is_never_accepted_as_local_workspace() {
        assert_eq!(
            authorize_runtime_workspace_dir(
                Some("user-1"),
                Some("harness://project/project-1".to_string()),
            )
            .await,
            None
        );
    }

    #[test]
    fn real_paths_remain_local_authorization_candidates() {
        assert_eq!(
            classify_runtime_project_root("/workspace/project-1"),
            RuntimeProjectRootKind::LocalCandidate("/workspace/project-1".to_string())
        );
        assert_eq!(
            classify_runtime_project_root("harness://project/"),
            RuntimeProjectRootKind::InvalidHarness
        );
    }
}

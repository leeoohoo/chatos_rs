// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::warn;

use super::support::normalize_optional_text;
use crate::api::fs::policy::FsPathPolicy;
use crate::core::auth::AuthUser;
use chatos_project_execution::{parse_local_connector_workspace_root, LOCAL_CONNECTOR_ROOT_PREFIX};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ResolvedRuntimeProjectRoot {
    pub(super) logical_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeProjectRootKind {
    LocalConnector(String),
    InvalidLocalConnector,
    Unsupported,
}

fn classify_runtime_project_root(raw: &str) -> RuntimeProjectRootKind {
    let raw = raw.trim().to_string();
    if parse_local_connector_workspace_root(raw.as_str()).is_some() {
        RuntimeProjectRootKind::LocalConnector(raw)
    } else if raw == "local://connector" || raw.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX) {
        RuntimeProjectRootKind::InvalidLocalConnector
    } else {
        RuntimeProjectRootKind::Unsupported
    }
}

pub(super) async fn resolve_runtime_project_root(
    raw: Option<String>,
) -> ResolvedRuntimeProjectRoot {
    let Some(raw) = normalize_optional_text(raw.as_deref()) else {
        return ResolvedRuntimeProjectRoot::default();
    };
    match classify_runtime_project_root(raw.as_str()) {
        RuntimeProjectRootKind::LocalConnector(logical_root) => ResolvedRuntimeProjectRoot {
            logical_root: Some(logical_root),
        },
        RuntimeProjectRootKind::InvalidLocalConnector => {
            warn!(
                project_root = raw.as_str(),
                "invalid Local Connector logical project root dropped"
            );
            ResolvedRuntimeProjectRoot::default()
        }
        RuntimeProjectRootKind::Unsupported => {
            warn!(
                project_root = raw.as_str(),
                "unsupported project root dropped; projects must use a Local Connector root"
            );
            ResolvedRuntimeProjectRoot::default()
        }
    }
}

pub(super) async fn authorize_runtime_workspace_dir(
    user_id: Option<&str>,
    raw: Option<String>,
) -> Option<String> {
    let raw = normalize_optional_text(raw.as_deref())?;
    if raw.contains("://") {
        warn!(
            workspace_dir = raw.as_str(),
            "logical project root is not a server-local workspace directory"
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
    async fn preserves_local_connector_root_as_logical_context_only() {
        let root = "local://connector/device-1/workspace-1/apps/backend";
        let resolved = resolve_runtime_project_root(Some(root.to_string())).await;

        assert_eq!(resolved.logical_root.as_deref(), Some(root));
    }

    #[tokio::test]
    async fn logical_root_is_never_accepted_as_server_local_workspace() {
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
    fn only_local_connector_roots_are_project_runtime_roots() {
        assert_eq!(
            classify_runtime_project_root("/workspace/project-1"),
            RuntimeProjectRootKind::Unsupported
        );
        assert_eq!(
            classify_runtime_project_root("harness://project/project-1"),
            RuntimeProjectRootKind::Unsupported
        );
        assert_eq!(
            classify_runtime_project_root("local://connector/device-only"),
            RuntimeProjectRootKind::InvalidLocalConnector
        );
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Component, Path, PathBuf};

#[path = "policy_paths.rs"]
mod policy_paths;
#[path = "policy_roots.rs"]
mod policy_roots;

use crate::core::auth::AuthUser;
use axum::http::StatusCode;

pub(crate) const PATH_OUTSIDE_ALLOWED_ROOTS: &str = "路径超出允许范围";
pub(crate) const PATH_TRAVERSAL_BLOCKED: &str = "路径不能包含 ..";
pub(crate) const WRITE_NOT_ALLOWED: &str = "当前目录不允许写入";

#[derive(Debug, Clone)]
pub(super) struct FsAllowedRoot {
    path: PathBuf,
    kind: FsAllowedRootKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FsAllowedRootKind {
    Workspace,
    Public,
    CurrentDir,
    RepoParent,
    Ssh,
    Home,
    Configured,
}

impl FsAllowedRootKind {
    fn priority(self) -> u8 {
        match self {
            Self::Workspace => 0,
            Self::Public => 1,
            Self::CurrentDir => 4,
            Self::RepoParent => 5,
            Self::Ssh => 6,
            Self::Home => 7,
            Self::Configured => 8,
        }
    }

    fn can_write(self) -> bool {
        matches!(
            self,
            Self::Workspace | Self::Public | Self::CurrentDir | Self::RepoParent | Self::Configured
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FsPathPolicy {
    roots: Vec<FsAllowedRoot>,
}

#[derive(Debug, Clone)]
pub(crate) struct AuthorizedPath {
    pub(crate) path: PathBuf,
    pub(crate) can_write: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum FsPolicyError {
    BadRequest(String),
    Forbidden(String),
    Internal(String),
}

impl FsPolicyError {
    pub(crate) fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(crate) fn message(&self) -> &str {
        match self {
            Self::BadRequest(message) | Self::Forbidden(message) | Self::Internal(message) => {
                message.as_str()
            }
        }
    }
}

impl FsPathPolicy {
    pub(crate) async fn for_user(auth: &AuthUser) -> Result<Self, FsPolicyError> {
        let roots = policy_roots::build_allowed_roots(auth).await;

        if roots.is_empty() {
            return Err(FsPolicyError::Forbidden(
                "当前用户没有可访问的本地目录".to_string(),
            ));
        }

        Ok(Self { roots })
    }

    pub(crate) fn authorize_existing_dir(
        &self,
        raw: &str,
        missing_message: &str,
        not_dir_message: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let authorized = self.authorize_existing_path_with_message(raw, missing_message)?;
        if !authorized.path.is_dir() {
            return Err(FsPolicyError::BadRequest(not_dir_message.to_string()));
        }
        Ok(authorized)
    }

    pub(crate) fn authorize_existing_file(
        &self,
        raw: &str,
        missing_message: &str,
        not_file_message: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let authorized = self.authorize_existing_path_with_message(raw, missing_message)?;
        if !authorized.path.is_file() {
            return Err(FsPolicyError::BadRequest(not_file_message.to_string()));
        }
        Ok(authorized)
    }

    pub(crate) fn require_write(&self, path: &AuthorizedPath) -> Result<(), FsPolicyError> {
        if !path.can_write {
            return Err(FsPolicyError::Forbidden(WRITE_NOT_ALLOWED.to_string()));
        }
        Ok(())
    }

    fn authorize_existing_path_with_message(
        &self,
        raw: &str,
        missing_message: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let resolved = self.resolve_input_path(raw)?;
        let canonical =
            policy_paths::canonicalize_existing_path(resolved.as_path(), missing_message)?;
        self.authorized_path_for(canonical)
    }

    fn resolve_input_path(&self, raw: &str) -> Result<PathBuf, FsPolicyError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(FsPolicyError::BadRequest("路径不能为空".to_string()));
        }
        if contains_parent_dir(Path::new(trimmed)) {
            return Err(FsPolicyError::Forbidden(PATH_TRAVERSAL_BLOCKED.to_string()));
        }

        let candidate = PathBuf::from(trimmed);
        if candidate.is_absolute()
            && (self.raw_path_points_inside_allowed_root(trimmed) || candidate.exists())
        {
            return policy_paths::resolve_input_path(raw);
        }

        if let Some(resolved) = self.resolve_user_visible_path(trimmed) {
            return Ok(resolved);
        }

        policy_paths::resolve_input_path(raw)
    }

    fn raw_path_points_inside_allowed_root(&self, raw: &str) -> bool {
        let candidate = PathBuf::from(raw);
        self.roots
            .iter()
            .any(|root| policy_paths::path_is_within_root(candidate.as_path(), root.path.as_path()))
    }

    fn resolve_user_visible_path(&self, raw: &str) -> Option<PathBuf> {
        let normalized = raw.trim().replace('\\', "/");
        if normalized.is_empty() {
            return None;
        }

        let public_root = self
            .roots
            .iter()
            .find(|root| root.kind == FsAllowedRootKind::Public)
            .map(|root| root.path.clone());
        let workspace_root = self
            .roots
            .iter()
            .find(|root| root.kind == FsAllowedRootKind::Workspace)
            .map(|root| root.path.clone());

        if normalized == "/public" {
            return public_root;
        }
        if let Some(relative) = normalized.strip_prefix("/public/") {
            return public_root.map(|root| root.join(relative));
        }
        if normalized == "/" {
            return workspace_root;
        }
        if let Some(relative) = normalized.strip_prefix('/') {
            return workspace_root.map(|root| root.join(relative));
        }
        if !Path::new(raw).is_absolute() {
            return workspace_root.map(|root| root.join(normalized));
        }
        None
    }

    fn authorized_path_for(&self, path: PathBuf) -> Result<AuthorizedPath, FsPolicyError> {
        let root = self
            .find_navigation_root(path.as_path())
            .ok_or_else(|| FsPolicyError::Forbidden(PATH_OUTSIDE_ALLOWED_ROOTS.to_string()))?;
        Ok(AuthorizedPath {
            path,
            can_write: root.kind.can_write(),
        })
    }

    fn find_navigation_root(&self, candidate: &Path) -> Option<&FsAllowedRoot> {
        self.roots
            .iter()
            .filter(|root| policy_paths::path_is_within_root(candidate, root.path.as_path()))
            .max_by_key(|root| policy_paths::normalize_path_for_compare(root.path.as_path()).len())
    }
}

fn contains_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

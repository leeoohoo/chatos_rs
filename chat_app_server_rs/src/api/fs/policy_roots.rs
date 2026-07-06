// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::core::auth::AuthUser;
use crate::models::project::ProjectService;
use crate::services::git::discover_repo_root;
use crate::utils::workspace::resolve_workspace_dir;

use super::super::roots::home_dir;
use super::policy_paths::{canonicalize_existing_dir, normalize_path_for_compare};
use super::{FsAllowedRoot, FsAllowedRootKind};

pub(super) async fn build_allowed_roots(auth: &AuthUser) -> Vec<FsAllowedRoot> {
    let mut roots = Vec::new();
    let host_roots_enabled = host_fs_roots_enabled();
    let allow_legacy_project_roots = legacy_project_roots_enabled() || host_roots_enabled;
    let user_roots = ensure_user_scoped_roots(auth);

    if let Some(user_roots) = user_roots.as_ref() {
        push_root(
            &mut roots,
            user_roots.workspaces_root.clone(),
            FsAllowedRootKind::Workspace,
        );
        push_root(
            &mut roots,
            user_roots.public_root.clone(),
            FsAllowedRootKind::Public,
        );
    }

    if host_roots_enabled {
        if let Ok(current_dir) = env::current_dir() {
            push_root(&mut roots, current_dir, FsAllowedRootKind::CurrentDir);
        }

        if let Ok(current_dir) = env::current_dir() {
            match discover_repo_root(current_dir.as_path()).await {
                Ok(Some(repo_root)) => {
                    if let Some(parent) = repo_root.parent() {
                        push_root(
                            &mut roots,
                            parent.to_path_buf(),
                            FsAllowedRootKind::RepoParent,
                        );
                    }
                }
                _ => {
                    if let Some(parent) = current_dir.parent() {
                        push_root(
                            &mut roots,
                            parent.to_path_buf(),
                            FsAllowedRootKind::RepoParent,
                        );
                    }
                }
            }
        }

        push_root(
            &mut roots,
            PathBuf::from(resolve_workspace_dir(None)),
            FsAllowedRootKind::Workspace,
        );

        if let Some(home) = home_dir() {
            push_root(&mut roots, home.join(".ssh"), FsAllowedRootKind::Ssh);
            push_root(&mut roots, home, FsAllowedRootKind::Home);
        }

        if let Ok(raw) = env::var("FS_ALLOWED_ROOTS") {
            for value in raw
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                push_root(
                    &mut roots,
                    PathBuf::from(value),
                    FsAllowedRootKind::Configured,
                );
            }
        }
    }

    if let Ok(projects) = ProjectService::list(Some(auth.user_id.clone())).await {
        for project in projects {
            let root = project.root_path.trim();
            if root.is_empty() {
                continue;
            }
            let root_path = PathBuf::from(root);
            let allowed_project_root = allow_legacy_project_roots
                || user_roots
                    .as_ref()
                    .is_some_and(|scope| path_is_within_user_scope(root_path.as_path(), scope));
            if !allowed_project_root {
                continue;
            }
            if allow_legacy_project_roots {
                if let Some(parent) = root_path.parent() {
                    push_root(
                        &mut roots,
                        parent.to_path_buf(),
                        FsAllowedRootKind::ProjectParent,
                    );
                }
            }
            push_root(&mut roots, root_path, FsAllowedRootKind::Project);
        }
    }

    roots.sort_by(|left, right| {
        left.kind
            .priority()
            .cmp(&right.kind.priority())
            .then_with(|| left.path.cmp(&right.path))
    });

    roots
}

#[derive(Debug, Clone)]
struct UserScopedRoots {
    user_root: PathBuf,
    workspaces_root: PathBuf,
    public_root: PathBuf,
}

fn ensure_user_scoped_roots(auth: &AuthUser) -> Option<UserScopedRoots> {
    let base = PathBuf::from(resolve_workspace_dir(None));
    let user_component = user_path_component(auth.user_id.as_str());
    let user_root = base.join("users").join(user_component);
    let workspaces_root = user_root.join("workspaces");
    let public_root = user_root.join("public");
    fs::create_dir_all(user_root.as_path()).ok()?;
    fs::create_dir_all(workspaces_root.as_path()).ok()?;
    fs::create_dir_all(public_root.as_path()).ok()?;
    set_private_dir_permissions(user_root.as_path()).ok()?;
    set_private_dir_permissions(workspaces_root.as_path()).ok()?;
    set_private_dir_permissions(public_root.as_path()).ok()?;
    Some(UserScopedRoots {
        user_root,
        workspaces_root,
        public_root,
    })
}

fn set_private_dir_permissions(_path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn path_is_within_user_scope(candidate: &Path, scope: &UserScopedRoots) -> bool {
    let Ok(user_root) = canonicalize_existing_dir(scope.user_root.as_path()) else {
        return false;
    };
    let Ok(candidate) = canonicalize_existing_dir(candidate) else {
        return false;
    };
    crate::core::path_guard::path_is_within_root(candidate.as_path(), user_root.as_path())
}

fn user_path_component(user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_id.as_bytes());
    let digest = hex::encode(hasher.finalize());
    let suffix = &digest[..16];
    let prefix = safe_path_component(user_id);
    format!("{prefix}-{suffix}")
}

fn safe_path_component(value: &str) -> String {
    let mut out = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while out.starts_with('.') {
        out.remove(0);
    }
    if out.is_empty() {
        "user".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{host_fs_roots_enabled_for, user_path_component};

    #[test]
    fn user_path_component_avoids_sanitization_collisions() {
        assert_ne!(user_path_component("a/b"), user_path_component("a_b"));
    }

    #[test]
    fn user_path_component_keeps_readable_prefix() {
        let value = user_path_component(" user-1 ");
        assert!(value.starts_with("user-1-"));
        assert!(value.len() > "user-1-".len());
    }

    #[test]
    fn host_fs_roots_default_to_enabled_outside_production() {
        assert!(host_fs_roots_enabled_for(None, None));
        assert!(host_fs_roots_enabled_for(Some("development"), None));
        assert!(host_fs_roots_enabled_for(Some("test"), None));
    }

    #[test]
    fn host_fs_roots_default_to_disabled_in_production() {
        assert!(!host_fs_roots_enabled_for(Some("production"), None));
        assert!(!host_fs_roots_enabled_for(Some(" production "), None));
    }

    #[test]
    fn host_fs_roots_explicit_env_overrides_default() {
        assert!(host_fs_roots_enabled_for(Some("production"), Some(true)));
        assert!(!host_fs_roots_enabled_for(Some("development"), Some(false)));
    }
}

fn host_fs_roots_enabled() -> bool {
    if let Some(value) = env_bool_override("CHATOS_ENABLE_HOST_FS_ROOTS")
        .or_else(|| env_bool_override("FS_ENABLE_HOST_ROOTS"))
    {
        return host_fs_roots_enabled_for(env::var("NODE_ENV").ok().as_deref(), Some(value));
    }
    host_fs_roots_enabled_for(env::var("NODE_ENV").ok().as_deref(), None)
}

fn host_fs_roots_enabled_for(node_env: Option<&str>, override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    !is_production_env_value(node_env)
}

fn legacy_project_roots_enabled() -> bool {
    env_bool("CHATOS_ALLOW_LEGACY_PROJECT_ROOTS")
}

fn is_production_env_value(value: Option<&str>) -> bool {
    value
        .map(|value| value.trim().eq_ignore_ascii_case("production"))
        .unwrap_or(false)
}

fn env_bool_override(key: &str) -> Option<bool> {
    env::var(key)
        .ok()
        .map(|value| matches_env_bool(value.trim()))
}

fn env_bool(key: &str) -> bool {
    env::var(key)
        .ok()
        .map(|value| matches_env_bool(value.trim()))
        .unwrap_or(false)
}

fn matches_env_bool(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn push_root(roots: &mut Vec<FsAllowedRoot>, candidate: PathBuf, kind: FsAllowedRootKind) {
    let Ok(canonical) = canonicalize_existing_dir(candidate.as_path()) else {
        return;
    };
    let normalized = normalize_path_for_compare(canonical.as_path());
    if roots
        .iter()
        .any(|root| normalize_path_for_compare(root.path.as_path()) == normalized)
    {
        return;
    }
    roots.push(FsAllowedRoot {
        path: canonical,
        kind,
    });
}

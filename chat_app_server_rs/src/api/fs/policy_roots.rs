use std::env;
use std::path::PathBuf;

use crate::core::auth::AuthUser;
use crate::models::project::ProjectService;
use crate::utils::workspace::resolve_workspace_dir;

use super::policy_paths::{canonicalize_existing_dir, normalize_path_for_compare};
use super::super::roots::home_dir;
use super::{FsAllowedRoot, FsAllowedRootKind};

pub(super) async fn build_allowed_roots(auth: &AuthUser) -> Vec<FsAllowedRoot> {
    let mut roots = Vec::new();

    if let Ok(current_dir) = env::current_dir() {
        push_root(&mut roots, current_dir, FsAllowedRootKind::CurrentDir);
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

    if let Ok(projects) = ProjectService::list(Some(auth.user_id.clone())).await {
        for project in projects {
            let root = project.root_path.trim();
            if root.is_empty() {
                continue;
            }
            push_root(&mut roots, PathBuf::from(root), FsAllowedRootKind::Project);
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

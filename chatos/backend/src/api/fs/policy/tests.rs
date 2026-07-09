// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::policy_paths::normalize_path_for_compare;
use super::FsPathPolicy;
use crate::core::auth::AuthUser;
use std::fs;
use std::path::{Path, PathBuf};

fn make_temp_dir(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("{}_{}", name, uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create temp dir");
    root
}

fn configured_policy(root: &Path) -> FsPathPolicy {
    FsPathPolicy {
        roots: vec![super::FsAllowedRoot {
            path: fs::canonicalize(root).expect("canonicalize root"),
            kind: super::FsAllowedRootKind::Configured,
        }],
    }
}

fn user_scoped_policy(root: &Path) -> FsPathPolicy {
    let workspaces = root.join("users").join("user-123").join("workspaces");
    let public = root.join("users").join("user-123").join("public");
    fs::create_dir_all(&workspaces).expect("create workspace root");
    fs::create_dir_all(&public).expect("create public root");
    FsPathPolicy {
        roots: vec![
            super::FsAllowedRoot {
                path: fs::canonicalize(workspaces).expect("canonicalize workspace root"),
                kind: super::FsAllowedRootKind::Workspace,
            },
            super::FsAllowedRoot {
                path: fs::canonicalize(public).expect("canonicalize public root"),
                kind: super::FsAllowedRootKind::Public,
            },
        ],
    }
}

fn mock_auth() -> AuthUser {
    AuthUser {
        user_id: "tester".to_string(),
        role: "user".to_string(),
    }
}

#[tokio::test]
async fn authorize_existing_path_rejects_parent_traversal() {
    let policy = FsPathPolicy::for_user(&mock_auth()).await;
    if let Ok(policy) = policy {
        let result = policy.authorize_existing_path("../outside");
        assert!(result.is_err());
    }
}

#[test]
fn normalize_path_compare_trims_trailing_slash() {
    assert_eq!(
        normalize_path_for_compare(PathBuf::from("/tmp/demo/").as_path()),
        "/tmp/demo"
    );
}

#[test]
fn compare_normalization_preserves_root() {
    assert_eq!(
        normalize_path_for_compare(PathBuf::from("/").as_path()),
        "/"
    );
}

#[test]
fn roots_json_returns_user_visible_paths_for_user_scoped_roots() {
    let root = make_temp_dir("fs_policy_visible_roots");
    let policy = user_scoped_policy(root.as_path());

    let roots = policy.roots_json();
    let paths = roots
        .iter()
        .filter_map(|value| value.get("path").and_then(|path| path.as_str()))
        .collect::<Vec<_>>();

    assert!(paths.contains(&"/"));
    assert!(paths.contains(&"/public"));
    assert!(paths.iter().all(|path| !path.contains("/users/user-123/")));

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn authorize_existing_path_accepts_user_visible_workspace_paths() {
    let root = make_temp_dir("fs_policy_visible_workspace");
    let demo = root
        .join("users")
        .join("user-123")
        .join("workspaces")
        .join("demo");
    fs::create_dir_all(&demo).expect("create demo");
    let policy = user_scoped_policy(root.as_path());
    let expected = fs::canonicalize(&demo).expect("canonicalize demo");

    let authorized = policy
        .authorize_existing_dir("/demo", "missing", "not dir")
        .expect("authorize virtual workspace path");
    assert_eq!(
        normalize_path_for_compare(authorized.path.as_path()),
        normalize_path_for_compare(expected.as_path())
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn authorize_existing_path_accepts_user_visible_public_paths() {
    let root = make_temp_dir("fs_policy_visible_public");
    let keys = root
        .join("users")
        .join("user-123")
        .join("public")
        .join("keys");
    fs::create_dir_all(&keys).expect("create keys");
    let policy = user_scoped_policy(root.as_path());
    let expected = fs::canonicalize(&keys).expect("canonicalize keys");

    let authorized = policy
        .authorize_existing_dir("/public/keys", "missing", "not dir")
        .expect("authorize virtual public path");
    assert_eq!(
        normalize_path_for_compare(authorized.path.as_path()),
        normalize_path_for_compare(expected.as_path())
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn expand_user_visible_path_maps_existing_virtual_input() {
    let root = make_temp_dir("fs_policy_expand_visible");
    let bin = root
        .join("users")
        .join("user-123")
        .join("workspaces")
        .join("demo")
        .join(".venv")
        .join("bin");
    fs::create_dir_all(&bin).expect("create bin");
    let python = bin.join("python");
    fs::write(&python, "").expect("create python");
    let policy = user_scoped_policy(root.as_path());

    let expanded = policy
        .expand_user_visible_path("/demo/.venv/bin/python")
        .expect("expand virtual path");
    assert_eq!(
        normalize_path_for_compare(Path::new(expanded.as_str())),
        normalize_path_for_compare(python.as_path())
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn exact_allowed_root_detection_uses_normalized_paths() {
    let root = make_temp_dir("fs_policy_root");
    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    let policy = configured_policy(root.as_path());
    assert!(policy.is_exact_allowed_root(root.join(".").as_path()) == false);
    assert!(policy.is_exact_allowed_root(canonical_root.as_path()));
    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn parent_for_returns_navigation_root_for_nested_directory() {
    let root = make_temp_dir("fs_policy_parent");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("create child");
    let policy = configured_policy(root.as_path());
    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    let canonical_child = fs::canonicalize(&child).expect("canonicalize child");
    let path = super::AuthorizedPath {
        path: canonical_child,
        navigation_root: canonical_root.clone(),
        project_root: Some(canonical_root.clone()),
        can_write: true,
    };

    assert_eq!(
        policy.parent_for(&path).as_deref(),
        Some(canonical_root.to_string_lossy().as_ref())
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn navigation_root_prefers_most_specific_allowed_root() {
    let root = make_temp_dir("fs_policy_specific_root");
    let nested_root = root.join(".ssh");
    let nested_child = nested_root.join("config");
    fs::create_dir_all(&nested_child).expect("create nested child");

    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    let canonical_nested_root = fs::canonicalize(&nested_root).expect("canonicalize nested root");
    let canonical_nested_child =
        fs::canonicalize(&nested_child).expect("canonicalize nested child");

    let policy = FsPathPolicy {
        roots: vec![
            super::FsAllowedRoot {
                path: canonical_root,
                kind: super::FsAllowedRootKind::Home,
            },
            super::FsAllowedRoot {
                path: canonical_nested_root.clone(),
                kind: super::FsAllowedRootKind::Ssh,
            },
        ],
    };

    let authorized = policy
        .authorize_existing_dir(
            canonical_nested_child.to_string_lossy().as_ref(),
            "路径不存在",
            "路径不是目录",
        )
        .expect("authorize nested dir");
    assert_eq!(authorized.navigation_root, canonical_nested_root);

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn write_permission_depends_on_allowed_root_kind() {
    let root = make_temp_dir("fs_policy_write_root");
    let writable = root.join("workspace");
    let readonly = root.join(".ssh");
    fs::create_dir_all(&writable).expect("create writable root");
    fs::create_dir_all(&readonly).expect("create readonly root");

    let canonical_writable = fs::canonicalize(&writable).expect("canonicalize writable");
    let canonical_readonly = fs::canonicalize(&readonly).expect("canonicalize readonly");
    let policy = FsPathPolicy {
        roots: vec![
            super::FsAllowedRoot {
                path: canonical_writable.clone(),
                kind: super::FsAllowedRootKind::Workspace,
            },
            super::FsAllowedRoot {
                path: canonical_readonly.clone(),
                kind: super::FsAllowedRootKind::Ssh,
            },
        ],
    };

    let writable_path = policy
        .authorize_existing_dir(
            canonical_writable.to_string_lossy().as_ref(),
            "路径不存在",
            "路径不是目录",
        )
        .expect("authorize writable");
    let readonly_path = policy
        .authorize_existing_dir(
            canonical_readonly.to_string_lossy().as_ref(),
            "路径不存在",
            "路径不是目录",
        )
        .expect("authorize readonly");

    assert!(policy.require_write(&writable_path).is_ok());
    assert!(matches!(
        policy.require_write(&readonly_path),
        Err(super::FsPolicyError::Forbidden(_))
    ));

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[test]
fn forbid_root_mutation_blocks_exact_allowed_root_only() {
    let root = make_temp_dir("fs_policy_forbid_root");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("create child");
    let policy = configured_policy(root.as_path());
    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    let canonical_child = fs::canonicalize(&child).expect("canonicalize child");

    assert!(policy
        .forbid_root_mutation(canonical_root.as_path())
        .is_err());
    assert!(policy
        .forbid_root_mutation(canonical_child.as_path())
        .is_ok());

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

#[cfg(unix)]
#[test]
fn authorize_existing_path_rejects_symlink_escape_for_read_access() {
    use std::os::unix::fs::symlink;

    let root = make_temp_dir("fs_policy_symlink_root");
    let outside = make_temp_dir("fs_policy_symlink_outside");
    let outside_file = outside.join("secret.txt");
    fs::write(&outside_file, "secret").expect("write outside file");
    let link = root.join("secret-link");
    symlink(&outside_file, &link).expect("create symlink");

    let policy = configured_policy(root.as_path());

    let result = policy.authorize_existing_path(link.to_string_lossy().as_ref());
    assert!(matches!(result, Err(super::FsPolicyError::Forbidden(_))));

    fs::remove_dir_all(root).expect("cleanup root");
    fs::remove_dir_all(outside).expect("cleanup outside");
}

#[cfg(unix)]
#[test]
fn authorize_existing_entry_allows_symlink_itself_for_mutation() {
    use std::os::unix::fs::symlink;

    let root = make_temp_dir("fs_policy_entry_root");
    let outside = make_temp_dir("fs_policy_entry_outside");
    let outside_file = outside.join("secret.txt");
    fs::write(&outside_file, "secret").expect("write outside file");
    let link = root.join("secret-link");
    symlink(&outside_file, &link).expect("create symlink");

    let policy = configured_policy(root.as_path());
    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    let expected_link = canonical_root.join("secret-link");

    let authorized = policy
        .authorize_existing_entry(link.to_string_lossy().as_ref(), "路径不存在", "路径不合法")
        .expect("authorize symlink entry");
    assert_eq!(authorized.path, expected_link);
    assert_eq!(authorized.navigation_root, canonical_root);

    fs::remove_dir_all(root).expect("cleanup root");
    fs::remove_dir_all(outside).expect("cleanup outside");
}

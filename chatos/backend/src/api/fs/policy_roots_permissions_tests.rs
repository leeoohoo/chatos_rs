// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::push_root;
use crate::api::fs::policy::{FsAllowedRootKind, FsPathPolicy, FsPolicyError, WRITE_NOT_ALLOWED};
use std::fs;
use std::path::{Path, PathBuf};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("chatos-fs-permissions-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        let path = fs::canonicalize(path).unwrap();
        fs::write(path.join("note.txt"), "unchanged").unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn assert_access(policy: &FsPathPolicy, path: &Path, writable: bool) {
    let directory = policy
        .authorize_existing_dir(path.to_str().unwrap(), "missing", "not dir")
        .unwrap();
    let file = policy
        .authorize_existing_file(
            path.join("note.txt").to_str().unwrap(),
            "missing",
            "not file",
        )
        .unwrap();
    for authorized in [directory, file] {
        assert_eq!(authorized.can_write, writable, "{path:?}");
        if writable {
            policy.require_write(&authorized).unwrap();
        } else {
            assert!(matches!(
                policy.require_write(&authorized),
                Err(FsPolicyError::Forbidden(ref message)) if message == WRITE_NOT_ALLOWED
            ));
        }
    }
    assert_eq!(
        fs::read_to_string(path.join("note.txt")).unwrap(),
        "unchanged"
    );
}

#[test]
fn duplicate_roots_preserve_read_only_permissions_in_both_orders() {
    let fixture = Fixture::new();
    for read_only in [FsAllowedRootKind::Home, FsAllowedRootKind::Ssh] {
        for writable in [
            FsAllowedRootKind::Workspace,
            FsAllowedRootKind::Public,
            FsAllowedRootKind::CurrentDir,
            FsAllowedRootKind::RepoParent,
            FsAllowedRootKind::Configured,
        ] {
            for kinds in [[writable, read_only], [read_only, writable]] {
                let mut roots = Vec::new();
                for kind in kinds {
                    push_root(&mut roots, fixture.0.clone(), kind);
                }
                assert_eq!(roots.len(), 1);
                // Keep the first root's navigation role/virtual path mapping.
                assert_eq!(roots[0].kind, kinds[0]);
                let policy = FsPathPolicy { roots };
                assert_access(&policy, &fixture.0, false);
                if writable == FsAllowedRootKind::Workspace && kinds[0] == writable {
                    let file = policy
                        .authorize_existing_file("note.txt", "missing", "not file")
                        .unwrap();
                    assert!(!file.can_write);
                }
            }
        }
    }
}

#[test]
fn distinct_nested_roots_keep_most_specific_permissions() {
    let fixture = Fixture::new();
    for (parent_kind, child_kind, child_writable) in [
        (
            FsAllowedRootKind::RepoParent,
            FsAllowedRootKind::Home,
            false,
        ),
        (FsAllowedRootKind::Home, FsAllowedRootKind::Workspace, true),
    ] {
        let child = fixture.0.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("note.txt"), "unchanged").unwrap();
        let mut roots = Vec::new();
        push_root(&mut roots, fixture.0.clone(), parent_kind);
        push_root(&mut roots, child.clone(), child_kind);
        let policy = FsPathPolicy { roots };
        assert_access(&policy, &fixture.0, parent_kind.can_write());
        assert_access(&policy, &child, child_writable);
    }
}

#[cfg(unix)]
#[test]
fn canonical_root_alias_cannot_override_read_only_permissions() {
    let fixture = Fixture::new();
    let alias = fixture.0.join("alias");
    std::os::unix::fs::symlink(&fixture.0, &alias).unwrap();
    let mut roots = Vec::new();
    push_root(&mut roots, alias.clone(), FsAllowedRootKind::CurrentDir);
    push_root(&mut roots, fixture.0.clone(), FsAllowedRootKind::Ssh);
    assert_eq!(roots.len(), 1);
    let policy = FsPathPolicy { roots };
    assert_access(&policy, &fixture.0, false);
    assert_access(&policy, &alias, false);
}

#[cfg(unix)]
#[test]
fn distinct_backslash_roots_cannot_discard_read_only_permissions() {
    let fixture = Fixture::new();
    let paths = [
        fixture.0.join(r"home\private"),
        fixture.0.join("home/private"),
    ];
    for path in &paths {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("note.txt"), "unchanged").unwrap();
    }
    assert_eq!(
        super::normalize_path_for_compare(&paths[0]),
        super::normalize_path_for_compare(&paths[1])
    );
    for kinds in [
        [FsAllowedRootKind::Home, FsAllowedRootKind::Home],
        [FsAllowedRootKind::Ssh, FsAllowedRootKind::Ssh],
        [FsAllowedRootKind::Home, FsAllowedRootKind::Ssh],
        [FsAllowedRootKind::Ssh, FsAllowedRootKind::Home],
    ] {
        for order in [[0, 1], [1, 0]] {
            let mut roots = Vec::new();
            push_root(&mut roots, fixture.0.clone(), FsAllowedRootKind::RepoParent);
            for index in order {
                push_root(&mut roots, paths[index].clone(), kinds[index]);
            }
            let policy = FsPathPolicy { roots };
            for path in &paths {
                assert_access(&policy, path, false);
            }
            assert_access(&policy, &fixture.0, true);
            assert_eq!(policy.roots.len(), 3);
        }
    }
}

#[cfg(unix)]
#[test]
fn private_permissions_reject_replaced_leaf_without_chmod_target() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let fixture = Fixture::new();
    for target_is_dir in [true, false] {
        let target = fixture
            .0
            .join(if target_is_dir { "directory" } else { "file" });
        if target_is_dir {
            fs::create_dir(&target).unwrap();
        } else {
            fs::write(&target, "unchanged").unwrap();
        }
        fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
        let before = fs::metadata(&target).unwrap().permissions();
        let checked = super::ensure_child_directory(&fixture.0, "workspaces").unwrap();
        // Deterministically model replacement after validation, before chmod.
        fs::remove_dir(&checked).unwrap();
        symlink(&target, &checked).unwrap();
        let result = super::set_private_dir_permissions(&checked);
        assert_eq!(
            fs::metadata(&target).unwrap().permissions(),
            before,
            "a replaced leaf must not change its symlink target's permissions"
        );
        assert!(result.is_err(), "a symlink must fail closed");
        if target_is_dir {
            assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        } else {
            assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
        }
        fs::remove_file(checked).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn private_permissions_reject_replaced_ancestors_without_chmod_target() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    for ancestor in ["users", "users/user", "users/user/workspaces"] {
        let fixture = Fixture::new();
        let checked = fixture.0.join("users/user/workspaces/nested");
        fs::create_dir_all(checked.parent().unwrap()).unwrap();
        let checked = super::ensure_child_directory(checked.parent().unwrap(), "nested").unwrap();
        let original = fixture.0.join(ancestor);
        let moved = fixture.0.join("moved");
        fs::rename(&original, &moved).unwrap();
        let target = moved.join(checked.strip_prefix(&original).unwrap());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
        fs::write(target.join("sentinel"), "unchanged").unwrap();
        let before = fs::metadata(&target).unwrap().permissions();
        // Substitute a validated ancestor before chmod, without timing a race.
        symlink(&moved, &original).unwrap();
        let result = super::set_private_dir_permissions(&checked);
        assert_eq!(
            fs::metadata(&target).unwrap().permissions(),
            before,
            "replaced ancestor {ancestor} must not redirect chmod"
        );
        assert!(result.is_err(), "ancestor symlinks must fail closed");
        assert_eq!(
            fs::read_to_string(target.join("sentinel")).unwrap(),
            "unchanged"
        );
    }
}

#[cfg(unix)]
#[test]
fn child_creation_rejects_replaced_ancestors_without_creating_in_target() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    for ancestor in ["users", "users/user", "users/user/workspaces"] {
        let fixture = Fixture::new();
        let mut parent = fixture.0.clone();
        for name in ["users", "user", "workspaces"] {
            parent = super::ensure_child_directory(&parent, name).unwrap();
        }
        let original = fixture.0.join(ancestor);
        let moved = fixture.0.join("moved");
        fs::rename(&original, &moved).unwrap();
        let target = moved.join(parent.strip_prefix(&original).unwrap());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
        fs::write(target.join("sentinel"), "unchanged").unwrap();
        let before = fs::metadata(&target).unwrap().permissions();
        // Replace an already validated ancestor before creating a descendant.
        symlink(&moved, &original).unwrap();
        let result = super::ensure_child_directory(&parent, "nested");
        assert!(
            !target.join("nested").exists(),
            "replaced ancestor {ancestor} must not redirect directory creation"
        );
        assert!(result.is_err(), "ancestor symlinks must fail closed");
        assert_eq!(fs::metadata(&target).unwrap().permissions(), before);
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
        assert_eq!(
            fs::read_to_string(target.join("sentinel")).unwrap(),
            "unchanged"
        );
    }
}

#[cfg(unix)]
#[test]
fn child_creation_requires_a_single_normal_name() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent");
    fs::create_dir(&parent).unwrap();
    let absolute = fixture.0.join("absolute");
    for name in [
        "",
        ".",
        "..",
        "../escape",
        "nested/child",
        "child/",
        absolute.to_str().unwrap(),
        "nul\0child",
    ] {
        assert!(super::ensure_child_directory(&parent, name).is_err());
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
    }
    let child = super::ensure_child_directory(&parent, "child").unwrap();
    assert_eq!(child, parent.join("child"));
    assert_eq!(
        super::ensure_child_directory(&parent, "child").unwrap(),
        child
    );
}

#[cfg(unix)]
#[test]
fn private_permissions_only_accept_existing_real_directories() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let fixture = Fixture::new();
    let directory = fixture.0.join("directory");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
    super::set_private_dir_permissions(&directory).unwrap();
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );

    let file = fixture.0.join("note.txt");
    let before = fs::metadata(&file).unwrap().permissions();
    assert!(super::set_private_dir_permissions(&file).is_err());
    assert_eq!(fs::metadata(&file).unwrap().permissions(), before);
    assert_eq!(fs::read_to_string(&file).unwrap(), "unchanged");
    let missing = fixture.0.join("missing");
    assert!(super::set_private_dir_permissions(&missing).is_err());
    let dangling = fixture.0.join("dangling");
    symlink(&missing, &dangling).unwrap();
    assert!(super::set_private_dir_permissions(&dangling).is_err());
    assert!(!missing.exists());
}

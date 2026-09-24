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

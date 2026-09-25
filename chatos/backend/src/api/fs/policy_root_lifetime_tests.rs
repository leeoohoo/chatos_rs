// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ensure_child_directory, push_root, push_user_scoped_roots, UserScopedRoots};
use crate::api::fs::policy::{FsAllowedRootKind, FsPathPolicy, FsPolicyError};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn write_checks_reject_redirected_descendants_of_unchanged_user_roots() {
    use std::os::unix::fs::symlink;

    for replaced in ["nested", "nested/child", "nested/child/note.txt"] {
        for destination in ["outside", "readonly", "public", "missing"] {
            let base = std::env::temp_dir()
                .join(format!("chatos-write-redirect-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&base).unwrap();
            let fixture = Fixture(fs::canonicalize(base).unwrap());
            let prepared = UserScopedRoots::new(
                ensure_child_directory(&fixture.0, "workspaces").unwrap(),
                ensure_child_directory(&fixture.0, "public").unwrap(),
            )
            .unwrap();
            let workspace = &prepared.workspaces_root;
            fs::create_dir_all(workspace.join("nested/child")).unwrap();
            fs::write(workspace.join("nested/child/note.txt"), "original").unwrap();
            let target = match destination {
                "readonly" => workspace.join("readonly"),
                "public" => prepared.public_root.clone(),
                _ => fixture.0.join(destination),
            };
            if destination != "missing" {
                fs::create_dir_all(target.join("child")).unwrap();
                fs::write(target.join("note.txt"), "unchanged").unwrap();
                fs::write(target.join("child/note.txt"), "unchanged").unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
            }
            let mut roots = Vec::new();
            push_user_scoped_roots(&mut roots, &prepared);
            if destination == "readonly" {
                push_root(&mut roots, target.clone(), FsAllowedRootKind::Home);
            }
            let policy = FsPathPolicy { roots };
            let directory = policy
                .authorize_existing_dir("nested/child", "missing", "not dir")
                .unwrap();
            let file = policy
                .authorize_existing_file("nested/child/note.txt", "missing", "not file")
                .unwrap();
            // Existing allowed symlinks resolve to canonical targets at the
            // initial authorization boundary and remain supported.
            symlink(workspace.join("nested/child"), workspace.join("alias")).unwrap();
            let alias = policy
                .authorize_existing_file("alias/note.txt", "missing", "not file")
                .unwrap();
            assert_eq!(alias.path, file.path);
            for authorized in [&directory, &file, &alias] {
                policy.require_write(authorized).unwrap();
            }

            let replaced_path = workspace.join(replaced);
            fs::rename(&replaced_path, fixture.0.join("original")).unwrap();
            let stale = if replaced.ends_with("note.txt") {
                vec![&file, &alias]
            } else {
                vec![&directory, &file, &alias]
            };
            let link_target = if replaced.ends_with("note.txt") {
                target.join("note.txt")
            } else {
                target.clone()
            };
            symlink(&link_target, &replaced_path).unwrap();
            for authorized in &stale {
                let result = policy.require_write(authorized);
                assert!(
                    matches!(result, Err(FsPolicyError::Forbidden(_))),
                    "{replaced} -> {destination}: stale write allowed: {result:?}"
                );
            }
            // Disappearance also invalidates the earlier write grant.
            fs::remove_file(&replaced_path).unwrap();
            for authorized in stale {
                assert!(matches!(
                    policy.require_write(authorized),
                    Err(FsPolicyError::Forbidden(_))
                ));
            }
            let unchanged_root = policy
                .authorize_existing_dir(workspace.to_str().unwrap(), "missing", "not dir")
                .unwrap();
            policy.require_write(&unchanged_root).unwrap();
            if replaced.ends_with("note.txt") {
                policy.require_write(&directory).unwrap();
            }
            if destination != "missing" {
                assert_eq!(
                    fs::read_to_string(target.join("note.txt")).unwrap(),
                    "unchanged"
                );
                assert_eq!(
                    fs::read_to_string(target.join("child/note.txt")).unwrap(),
                    "unchanged"
                );
                assert_eq!(fs::read_dir(&target).unwrap().count(), 2);
                assert_eq!(
                    fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                    0o750
                );
            } else {
                assert!(!target.exists());
            }
        }
    }
}

#[test]
fn registered_user_roots_reject_later_real_directory_replacements() {
    for replaced in ["users", "alice", "workspaces", "public"] {
        for configured_parent in [false, true] {
            let base =
                std::env::temp_dir().join(format!("chatos-root-lifetime-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&base).unwrap();
            let fixture = Fixture(fs::canonicalize(base).unwrap());
            let users = ensure_child_directory(&fixture.0, "users").unwrap();
            let user = ensure_child_directory(&users, "alice").unwrap();
            let prepared = UserScopedRoots::new(
                ensure_child_directory(&user, "workspaces").unwrap(),
                ensure_child_directory(&user, "public").unwrap(),
            )
            .unwrap();
            let paths = [
                prepared.workspaces_root.clone(),
                prepared.public_root.clone(),
            ];
            let mut roots = Vec::new();
            push_user_scoped_roots(&mut roots, &prepared);
            if configured_parent {
                push_root(&mut roots, fixture.0.clone(), FsAllowedRootKind::Configured);
                // A duplicate configured entry must retain the user's handle.
                for path in &paths {
                    push_root(&mut roots, path.clone(), FsAllowedRootKind::Configured);
                }
                assert_eq!(roots.len(), 3);
            }
            let original_policy = FsPathPolicy { roots };
            let policy = original_policy.clone();
            // Production drops preparation state after building the policy.
            // Cloning a policy must retain the same identity protection.
            drop(prepared);
            drop(original_policy);
            let mut authorizations = Vec::new();
            for path in &paths {
                fs::write(path.join("note.txt"), "unchanged").unwrap();
                let authorized = policy
                    .authorize_existing_dir(path.to_str().unwrap(), "missing", "not dir")
                    .unwrap();
                policy.require_write(&authorized).unwrap();
                authorizations.push(authorized);
                let file = policy
                    .authorize_existing_file(
                        path.join("note.txt").to_str().unwrap(),
                        "missing",
                        "not file",
                    )
                    .unwrap();
                policy.require_write(&file).unwrap();
                authorizations.push(file);
            }
            for virtual_path in ["note.txt", "/public/note.txt"] {
                let authorized = policy
                    .authorize_existing_file(virtual_path, "missing", "not file")
                    .unwrap();
                policy.require_write(&authorized).unwrap();
                authorizations.push(authorized);
            }
            let replaced_path = match replaced {
                "users" => &users,
                "alice" => &user,
                "workspaces" => &paths[0],
                "public" => &paths[1],
                _ => unreachable!(),
            };
            let incoming = fixture.0.join("incoming");
            fs::create_dir(&incoming).unwrap();
            for path in &paths {
                if let Ok(relative) = path.strip_prefix(replaced_path) {
                    let target = incoming.join(relative);
                    fs::create_dir_all(&target).unwrap();
                    fs::write(target.join("note.txt"), "unchanged").unwrap();
                    fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
                }
            }
            let before = fs::metadata(replaced_path).unwrap();
            fs::rename(replaced_path, fixture.0.join("original")).unwrap();
            fs::rename(incoming, replaced_path).unwrap();
            let after = fs::metadata(replaced_path).unwrap();
            assert_ne!((before.dev(), before.ino()), (after.dev(), after.ino()));
            // Settings and runtime request write permission separately from
            // initial path authorization. Do not trust a stale writable result.
            for authorized in &authorizations {
                let result = policy.require_write(authorized);
                if authorized.path.starts_with(replaced_path) {
                    assert!(
                        matches!(result, Err(FsPolicyError::Forbidden(_))),
                        "{replaced}, configured_parent={configured_parent}: stale write allowed: {result:?}"
                    );
                } else {
                    result.unwrap();
                }
            }
            for (index, path) in paths.iter().enumerate() {
                assert_eq!(fs::canonicalize(path).unwrap(), *path);
                let results = [
                    policy.authorize_existing_dir(path.to_str().unwrap(), "missing", "not dir"),
                    policy.authorize_existing_file(
                        path.join("note.txt").to_str().unwrap(),
                        "missing",
                        "not file",
                    ),
                    policy.authorize_existing_file(
                        ["note.txt", "/public/note.txt"][index],
                        "missing",
                        "not file",
                    ),
                ];
                for result in results {
                    if path.starts_with(replaced_path) {
                        assert!(matches!(result, Err(FsPolicyError::Forbidden(_))),
                            "{replaced}, configured_parent={configured_parent}: replacement authorized: {result:?}");
                    } else {
                        policy.require_write(&result.unwrap()).unwrap();
                    }
                }
                assert_eq!(
                    fs::read_to_string(path.join("note.txt")).unwrap(),
                    "unchanged"
                );
                assert_eq!(fs::read_dir(path).unwrap().count(), 1);
                let expected_mode = if path.starts_with(replaced_path) {
                    0o750
                } else {
                    0o700
                };
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    expected_mode
                );
            }
            if configured_parent {
                let authorized = policy
                    .authorize_existing_dir(fixture.0.to_str().unwrap(), "missing", "not dir")
                    .unwrap();
                policy.require_write(&authorized).unwrap();
            }
        }
    }
}

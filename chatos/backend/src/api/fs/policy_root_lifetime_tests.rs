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
            for path in &paths {
                fs::write(path.join("note.txt"), "unchanged").unwrap();
                let authorized = policy
                    .authorize_existing_dir(path.to_str().unwrap(), "missing", "not dir")
                    .unwrap();
                policy.require_write(&authorized).unwrap();
            }
            for virtual_path in ["note.txt", "/public/note.txt"] {
                let authorized = policy
                    .authorize_existing_file(virtual_path, "missing", "not file")
                    .unwrap();
                policy.require_write(&authorized).unwrap();
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

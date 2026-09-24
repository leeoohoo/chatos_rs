// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{build_allowed_roots, user_path_component, FsAllowedRootKind};
use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::core::auth::AuthUser;
use std::fs;
use std::path::{Path, PathBuf};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("chatos-fs-roots-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn user_roots_are_private_at_creation() {
    use std::os::unix::fs::PermissionsExt;

    const CHILD: &str = "CHATOS_TEST_PRIVATE_ROOT_CREATION";
    if std::env::var_os(CHILD).is_some() {
        let base = PathBuf::from(std::env::var("CHATOS_WORKSPACE_DIR").unwrap());
        let mut parent = base.parent().unwrap().to_path_buf();
        // Check each component immediately, before the root builder's final
        // chmod pass. This also covers the shared users directory.
        for name in ["users", "alice", "workspaces", "nested"] {
            parent = super::ensure_child_directory(&parent, name).unwrap();
            assert_private(&parent);
        }

        let auth = AuthUser {
            user_id: "alice".into(),
            role: "user".into(),
        };
        let user_root = base.join("users").join(user_path_component(&auth.user_id));
        fs::create_dir_all(&user_root).unwrap();
        fs::set_permissions(&user_root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(user_root.join("public"), "not a directory").unwrap();
        // A later failure must not leave the newly created workspace exposed,
        // even though the final chmod pass is never reached.
        assert!(build_allowed_roots(&auth).await.is_empty());
        assert_private(&user_root.join("workspaces"));
        assert!(matches!(
            FsPathPolicy::for_user(&auth).await,
            Err(FsPolicyError::Forbidden(_))
        ));
        assert_private(&user_root);
        assert_eq!(
            fs::read_to_string(user_root.join("public")).unwrap(),
            "not a directory"
        );
        return;
    }

    let fixture = Fixture::new();
    for mask in ["000", "022", "077"] {
        let root = fixture.0.join(mask);
        fs::create_dir(&root).unwrap();
        let module = module_path!().split_once("::").unwrap().1;
        // Set umask before starting the test process; never mutate it in the
        // multithreaded parent test runner. Arguments are passed without eval.
        let output = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "umask \"$1\"; shift; exec \"$@\"",
                "private-root-test",
                mask,
            ])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{module}::user_roots_are_private_at_creation"),
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("CHATOS_WORKSPACE_DIR", root.join("failed-base"))
            .env_remove("CHATOS_ENABLE_HOST_FS_ROOTS")
            .env_remove("FS_ENABLE_HOST_ROOTS")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
        assert!(
            output.status.success(),
            "umask {mask}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn user_roots_reject_redirected_directories() {
    const CASE: &str = "CHATOS_TEST_ROOT_ISOLATION_CASE";
    if let Ok(case) = std::env::var(CASE) {
        check_case(&case).await;
        return;
    }
    let fixture = Fixture::new();
    let cases = [
        "normal",
        "file-users",
        "file-user",
        "file-workspaces",
        "file-public",
    ];
    #[cfg(unix)]
    let cases = [
        &cases[..],
        &[
            "link-users",
            "link-user",
            "link-workspaces",
            "link-public",
            "dangling-workspaces",
            "other-user",
            "inside-user",
            "configured-base-link",
            "backslash-sibling",
        ],
    ]
    .concat();
    for case in cases {
        let root = fixture.0.join(case);
        fs::create_dir(&root).unwrap();
        let module = module_path!().split_once("::").unwrap().1;
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{module}::user_roots_reject_redirected_directories"),
                "--nocapture",
            ])
            .env(CASE, case)
            .env("CHATOS_WORKSPACE_DIR", root.join("base"))
            .env("FS_ALLOWED_ROOTS", &root)
            .env_remove("CHATOS_ENABLE_HOST_FS_ROOTS")
            .env_remove("FS_ENABLE_HOST_ROOTS")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
        assert!(
            output.status.success(),
            "case {case}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

async fn check_case(case: &str) {
    let base = PathBuf::from(std::env::var("CHATOS_WORKSPACE_DIR").unwrap());
    let outside = base.parent().unwrap().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel"), "unchanged").unwrap();
    let original_permissions = fs::metadata(&outside).unwrap().permissions();
    let auth = AuthUser {
        user_id: "alice".into(),
        role: "user".into(),
    };
    let user_root = base.join("users").join(user_path_component(&auth.user_id));
    let target = match case {
        "link-users" | "file-users" => base.join("users"),
        "link-user" | "file-user" => user_root.clone(),
        "link-public" | "file-public" => user_root.join("public"),
        _ => user_root.join("workspaces"),
    };
    if case.starts_with("file-") {
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "not a directory").unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        if case.starts_with("link-") || case == "dangling-workspaces" {
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let destination = if case == "dangling-workspaces" {
                outside.join("missing")
            } else {
                outside.clone()
            };
            symlink(destination, &target).unwrap();
        } else if case == "other-user" || case == "inside-user" {
            let destination = if case == "other-user" {
                base.join("users")
                    .join(user_path_component("bob"))
                    .join("workspaces")
            } else {
                user_root.join("public")
            };
            fs::create_dir_all(&destination).unwrap();
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            symlink(destination, &target).unwrap();
        } else if case == "configured-base-link" {
            symlink(&outside, &base).unwrap();
        }
    }
    let roots = build_allowed_roots(&auth).await;
    if matches!(
        case,
        "normal" | "configured-base-link" | "backslash-sibling"
    ) {
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].kind, FsAllowedRootKind::Workspace);
        assert_eq!(roots[1].kind, FsAllowedRootKind::Public);
        assert_eq!(
            roots[0].path,
            fs::canonicalize(user_root.join("workspaces")).unwrap()
        );
        assert_eq!(
            roots[1].path,
            fs::canonicalize(user_root.join("public")).unwrap()
        );
        let policy = FsPathPolicy::for_user(&auth).await.unwrap();
        #[cfg(unix)]
        if case == "backslash-sibling" {
            check_backslash_sibling(&policy, &user_root);
        }
        for root in &roots {
            let authorized = policy
                .authorize_existing_dir(root.path.to_str().unwrap(), "missing", "not dir")
                .unwrap();
            policy.require_write(&authorized).unwrap();
            assert_private(&root.path);
        }
        assert_private(&user_root);
        assert!(matches!(
            policy.authorize_existing_dir(outside.to_str().unwrap(), "missing", "not dir"),
            Err(FsPolicyError::Forbidden(_))
        ));
    } else {
        assert!(
            roots.is_empty(),
            "redirected/non-directory root must fail closed: {roots:?}"
        );
        assert!(matches!(
            FsPathPolicy::for_user(&auth).await,
            Err(FsPolicyError::Forbidden(_))
        ));
        assert_eq!(
            fs::read_dir(&outside).unwrap().count(),
            1,
            "must not create directories in the target"
        );
        assert_eq!(
            fs::metadata(&outside).unwrap().permissions(),
            original_permissions,
            "must not chmod the target"
        );
    }
    assert_eq!(
        fs::read_to_string(outside.join("sentinel")).unwrap(),
        "unchanged"
    );
}

#[cfg(unix)]
fn check_backslash_sibling(policy: &FsPathPolicy, user_root: &Path) {
    use std::os::unix::fs::symlink;

    // On Unix this is a sibling directory, not a child of workspaces.
    let sibling = user_root.join(r"workspaces\private");
    fs::create_dir(&sibling).unwrap();
    let secret = sibling.join("secret.txt");
    fs::write(&secret, "outside the authorized root").unwrap();
    let link = user_root.join("workspaces/redirect");
    symlink(&sibling, &link).unwrap();
    for directory in [&sibling, &link] {
        assert!(
            matches!(
                policy.authorize_existing_dir(directory.to_str().unwrap(), "missing", "not dir"),
                Err(FsPolicyError::Forbidden(_))
            ),
            "a sibling or symlink to it must not be authorized: {directory:?}"
        );
        assert!(matches!(
            policy.authorize_existing_file(
                directory.join("secret.txt").to_str().unwrap(),
                "missing",
                "not file"
            ),
            Err(FsPolicyError::Forbidden(_))
        ));
    }

    // A literal backslash in an actual child filename remains valid.
    let child = user_root.join(r"workspaces/valid\child");
    fs::create_dir(&child).unwrap();
    let file = child.join("note.txt");
    fs::write(&file, "inside the authorized root").unwrap();
    let authorized = policy
        .authorize_existing_dir(child.to_str().unwrap(), "missing", "not dir")
        .unwrap();
    policy.require_write(&authorized).unwrap();
    let authorized = policy
        .authorize_existing_file(file.to_str().unwrap(), "missing", "not file")
        .unwrap();
    policy.require_write(&authorized).unwrap();
    assert_eq!(
        fs::read_to_string(secret).unwrap(),
        "outside the authorized root"
    );
}

fn assert_private(_path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(_path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

#[cfg(unix)]
#[test]
fn user_root_registration_rejects_replaced_directories() {
    use super::{ensure_child_directory, push_user_scoped_roots, UserScopedRoots};
    use std::os::unix::fs::{symlink, PermissionsExt};

    for replaced in ["users", "alice", "workspaces", "public"] {
        let fixture = Fixture::new();
        let users = ensure_child_directory(&fixture.0, "users").unwrap();
        let user = ensure_child_directory(&users, "alice").unwrap();
        let user_roots = UserScopedRoots {
            workspaces_root: ensure_child_directory(&user, "workspaces").unwrap(),
            public_root: ensure_child_directory(&user, "public").unwrap(),
        };
        for path in [&user, &user_roots.workspaces_root, &user_roots.public_root] {
            super::set_private_dir_permissions(path).unwrap();
        }
        let mut roots = Vec::new();
        push_user_scoped_roots(&mut roots, &user_roots);
        let original = FsPathPolicy { roots };
        assert_eq!(original.roots.len(), 2);
        for path in [&user_roots.workspaces_root, &user_roots.public_root] {
            let authorized = original
                .authorize_existing_dir(path.to_str().unwrap(), "missing", "not dir")
                .unwrap();
            original.require_write(&authorized).unwrap();
        }

        let replaced_path = match replaced {
            "users" => &users,
            "alice" => &user,
            "workspaces" => &user_roots.workspaces_root,
            "public" => &user_roots.public_root,
            _ => unreachable!(),
        };
        // Replace after validation/chmod but before registering the roots. No
        // thread scheduling or process-wide environment mutation is needed.
        let outside = fixture.0.join("outside");
        fs::create_dir(&outside).unwrap();
        for path in [&user_roots.workspaces_root, &user_roots.public_root] {
            if let Ok(relative) = path.strip_prefix(replaced_path) {
                let target = outside.join(relative);
                fs::create_dir_all(&target).unwrap();
                fs::write(target.join("note.txt"), "unchanged").unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
            }
        }
        fs::rename(replaced_path, fixture.0.join("original")).unwrap();
        symlink(&outside, replaced_path).unwrap();
        let mut roots = Vec::new();
        push_user_scoped_roots(&mut roots, &user_roots);
        let policy = FsPathPolicy { roots };
        for path in [&user_roots.workspaces_root, &user_roots.public_root] {
            if let Ok(relative) = path.strip_prefix(replaced_path) {
                let target = outside.join(relative);
                for candidate in [path, &target] {
                    assert!(
                        matches!(
                            policy.authorize_existing_dir(
                                candidate.to_str().unwrap(),
                                "missing",
                                "not dir"
                            ),
                            Err(FsPolicyError::Forbidden(_))
                        ),
                        "{replaced}: redirected directory was authorized: {candidate:?}"
                    );
                    assert!(matches!(
                        policy.authorize_existing_file(
                            candidate.join("note.txt").to_str().unwrap(),
                            "missing",
                            "not file"
                        ),
                        Err(FsPolicyError::Forbidden(_))
                    ));
                }
                assert_eq!(
                    fs::read_to_string(target.join("note.txt")).unwrap(),
                    "unchanged"
                );
                assert_eq!(
                    fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                    0o750
                );
            } else {
                let authorized = policy
                    .authorize_existing_dir(path.to_str().unwrap(), "missing", "not dir")
                    .unwrap();
                policy.require_write(&authorized).unwrap();
            }
        }
        assert_eq!(
            policy.roots.len(),
            usize::from(matches!(replaced, "workspaces" | "public"))
        );
    }
}

#[cfg(unix)]
#[test]
fn child_validation_rejects_unix_path_alias_redirects() {
    use super::{ensure_child_directory, normalize_path_for_compare, validate_child_directory};
    use std::os::unix::fs::{symlink, PermissionsExt};

    for reverse in [false, true] {
        for relative in [
            "users",
            "users/alice",
            "users/alice/workspaces",
            "users/alice/public",
        ] {
            let fixture = Fixture::new();
            let literal = fixture.0.join(r"workspace\base");
            let separated = fixture.0.join("workspace/base");
            let (base, outside) = if reverse {
                (separated, literal)
            } else {
                (literal, separated)
            };
            fs::create_dir_all(&base).unwrap();
            fs::create_dir_all(&outside).unwrap();
            let mut path = base.clone();
            let mut target = outside.clone();
            for component in Path::new(relative).components() {
                let name = component.as_os_str().to_str().unwrap();
                path = ensure_child_directory(&path, name).unwrap();
                target = ensure_child_directory(&target, name).unwrap();
            }
            // Legitimate literal-backslash directories remain usable.
            assert_eq!(validate_child_directory(&path).unwrap(), path);
            assert_eq!(validate_child_directory(&target).unwrap(), target);
            assert_ne!(path, target);
            assert_eq!(
                normalize_path_for_compare(&path),
                normalize_path_for_compare(&target)
            );
            fs::write(target.join("sentinel"), "unchanged").unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();
            // Deterministically replace an ancestor after mkdirat, before the
            // same post-creation validation used by ensure_child_directory.
            fs::rename(&base, fixture.0.join("original")).unwrap();
            symlink(&outside, &base).unwrap();
            assert!(fs::symlink_metadata(&path).unwrap().file_type().is_dir());
            assert_eq!(fs::canonicalize(&path).unwrap(), target);
            let result = validate_child_directory(&path);
            assert!(
                matches!(&result, Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied),
                "redirected canonical path must not become a validated user root: {result:?}"
            );
            assert_eq!(
                fs::read_to_string(target.join("sentinel")).unwrap(),
                "unchanged"
            );
            assert_eq!(
                fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o750
            );
            assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
            assert!(validate_child_directory(&target).is_ok());
        }
    }
}

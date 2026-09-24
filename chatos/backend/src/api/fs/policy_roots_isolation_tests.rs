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

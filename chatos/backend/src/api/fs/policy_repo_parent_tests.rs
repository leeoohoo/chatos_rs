// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{build_allowed_roots, log_host_fs_roots_configuration, FsAllowedRootKind};
use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::core::auth::AuthUser;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[tokio::test]
async fn repo_parent_requires_separate_opt_in() {
    const CHILD: &str = "CHATOS_TEST_REPO_PARENT_ROOT";
    const EXPECTED: &str = "CHATOS_TEST_REPO_PARENT_ENABLED";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let enabled = std::env::var(EXPECTED).unwrap() == "true";
        let auth = AuthUser {
            user_id: "repo-parent-test".into(),
            role: "user".into(),
        };
        let roots = build_allowed_roots(&auth).await;
        let policy = FsPathPolicy::for_user(&auth).await.unwrap();
        let sibling = root.join("parent/sibling");
        let directory =
            policy.authorize_existing_dir(sibling.to_str().unwrap(), "missing", "not dir");
        let file = policy.authorize_existing_file(
            sibling.join("sentinel").to_str().unwrap(),
            "missing",
            "not file",
        );
        for result in [directory, file] {
            if enabled {
                policy.require_write(&result.unwrap()).unwrap();
            } else {
                assert!(
                    matches!(result, Err(FsPolicyError::Forbidden(_))),
                    "unexpected sibling authorization: {result:?}"
                );
            }
        }
        assert_eq!(
            roots
                .iter()
                .any(|root| root.kind == FsAllowedRootKind::RepoParent),
            enabled,
            "repository parent must require both explicit switches"
        );
        // The new gate must not remove the user's ordinary workspace access.
        let workspace = policy
            .authorize_existing_dir(".", "missing", "not dir")
            .unwrap();
        policy.require_write(&workspace).unwrap();
        assert_eq!(
            fs::read_to_string(sibling.join("sentinel")).unwrap(),
            "unchanged"
        );
        assert_eq!(fs::read_dir(&sibling).unwrap().count(), 1);

        let capture = Capture::default();
        let writer = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .json()
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, log_host_fs_roots_configuration);
        let bytes = capture.0.lock().unwrap().clone();
        let events = String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|event| event["fields"]["event"] == "repo_parent_fs_root_enabled")
            .collect::<Vec<_>>();
        assert_eq!(events.len(), usize::from(enabled));
        if enabled {
            assert_eq!(events[0]["level"], "WARN");
            assert_eq!(
                events[0]["fields"],
                serde_json::json!({
                    "event": "repo_parent_fs_root_enabled",
                    "message": "Repository parent filesystem root explicitly enabled",
                })
            );
        }
        return;
    }

    let root = std::env::temp_dir().join(format!("chatos-repo-parent-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let fixture = Fixture(fs::canonicalize(root).unwrap());
    let values = [
        (None, false),
        (Some(""), false),
        (Some("false"), false),
        (Some("invalid"), false),
        (Some("true"), true),
        (Some(" TRUE "), true),
        (Some("1"), true),
        (Some("yes"), true),
        (Some("on"), true),
    ]
    .map(|(value, enabled)| (value.map(std::ffi::OsString::from), enabled));
    #[cfg(unix)]
    let values = {
        use std::os::unix::ffi::OsStringExt;
        [
            values.to_vec(),
            vec![(
                Some(std::ffi::OsString::from_vec(b"\xfftrue".to_vec())),
                false,
            )],
        ]
        .concat()
    };
    for with_git in [true, false] {
        for host in ["primary", "legacy", "disabled"] {
            for (index, (value, opted_in)) in values.iter().enumerate() {
                let root = fixture.0.join(format!("{with_git}-{host}-{index}"));
                let repo = root.join("parent/repo");
                fs::create_dir_all(&repo).unwrap();
                let cwd = if with_git {
                    fs::create_dir(repo.join(".git")).unwrap();
                    let nested = repo.join("nested");
                    fs::create_dir(&nested).unwrap();
                    nested
                } else {
                    repo
                };
                fs::create_dir(root.join("parent/sibling")).unwrap();
                fs::write(root.join("parent/sibling/sentinel"), "unchanged").unwrap();
                fs::create_dir(root.join("fake-home")).unwrap();
                let enabled = host != "disabled" && *opted_in;
                let module = module_path!().split_once("::").unwrap().1;
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command
                    .args([
                        "--exact",
                        &format!("{module}::repo_parent_requires_separate_opt_in"),
                        "--nocapture",
                    ])
                    .current_dir(cwd)
                    .env(CHILD, &root)
                    .env(EXPECTED, enabled.to_string())
                    .env("CHATOS_WORKSPACE_DIR", root.join("workspace"))
                    .env_remove("HOME")
                    .env("USERPROFILE", root.join("fake-home"))
                    .env_remove("HOMEDRIVE")
                    .env_remove("HOMEPATH")
                    .env_remove("FS_ALLOWED_ROOTS")
                    .env_remove("CHATOS_ENABLE_HOST_FS_ROOTS")
                    .env_remove("FS_ENABLE_HOST_ROOTS")
                    .env_remove("CHATOS_ENABLE_REPO_PARENT_FS_ROOT")
                    .env_remove("CHATOS_ENABLE_HOME_FS_ROOTS");
                match host {
                    "primary" => {
                        command.env("CHATOS_ENABLE_HOST_FS_ROOTS", "true");
                    }
                    "legacy" => {
                        command.env("FS_ENABLE_HOST_ROOTS", "true");
                    }
                    _ => {
                        command
                            .env("CHATOS_ENABLE_HOST_FS_ROOTS", "false")
                            .env("FS_ENABLE_HOST_ROOTS", "true");
                    }
                }
                if let Some(value) = value {
                    command.env("CHATOS_ENABLE_REPO_PARENT_FS_ROOT", value);
                }
                let output = command.output().unwrap();
                assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
                assert!(
                    output.status.success(),
                    "git={with_git}, host={host}, value={value:?}:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

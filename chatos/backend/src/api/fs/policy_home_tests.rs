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
async fn home_roots_require_separate_opt_in() {
    const CHILD: &str = "CHATOS_TEST_HOME_ROOT";
    const ENABLED: &str = "CHATOS_TEST_HOME_ENABLED";
    const OVERLAP: &str = "CHATOS_TEST_HOME_OVERLAP";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let enabled = std::env::var(ENABLED).unwrap() == "true";
        let overlap = std::env::var(OVERLAP).unwrap();
        let linked = std::env::var("CHATOS_TEST_HOME_LINKED").unwrap() == "true";
        let host = std::env::var("CHATOS_TEST_HOME_HOST").unwrap() != "disabled";
        let auth = AuthUser {
            user_id: "home-root-test".into(),
            role: "user".into(),
        };
        let roots = build_allowed_roots(&auth).await;
        let policy = FsPathPolicy::for_user(&auth).await.unwrap();
        let home = root.join("private/home");
        for (relative, kind, covered, writable) in [
            (
                "",
                FsAllowedRootKind::Home,
                matches!(overlap.as_str(), "parent" | "home"),
                false,
            ),
            (
                ".ssh",
                FsAllowedRootKind::Ssh,
                overlap == "ssh" || (!linked && matches!(overlap.as_str(), "parent" | "home")),
                false,
            ),
            (
                "project",
                FsAllowedRootKind::Configured,
                overlap != "none" && overlap != "ssh",
                overlap == "child",
            ),
        ] {
            let directory = home.join(relative);
            let allowed = enabled || (host && covered);
            for result in [
                policy.authorize_existing_dir(directory.to_str().unwrap(), "missing", "not dir"),
                policy.authorize_existing_file(
                    directory.join("sentinel").to_str().unwrap(),
                    "missing",
                    "not file",
                ),
            ] {
                if allowed {
                    let path = result.unwrap();
                    assert_eq!(path.can_write, writable, "permission changed: {relative}");
                    assert_eq!(policy.require_write(&path).is_ok(), writable);
                } else {
                    assert!(
                        matches!(result, Err(FsPolicyError::Forbidden(_))),
                        "unexpected home access: {relative}: {result:?}"
                    );
                }
            }
            if kind != FsAllowedRootKind::Configured {
                assert_eq!(roots.iter().any(|root| root.kind == kind), allowed);
            }
            assert_eq!(
                fs::read_to_string(directory.join("sentinel")).unwrap(),
                "unchanged"
            );
        }
        assert_eq!(fs::read_dir(&home).unwrap().count(), 3);
        assert_eq!(fs::read_dir(home.join(".ssh")).unwrap().count(), 1);
        assert_eq!(fs::read_dir(home.join("project")).unwrap().count(), 1);
        let workspace = policy
            .authorize_existing_dir(".", "missing", "not dir")
            .unwrap();
        policy.require_write(&workspace).unwrap();

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
            .filter(|event| event["fields"]["event"] == "home_fs_roots_enabled")
            .collect::<Vec<_>>();
        assert_eq!(events.len(), usize::from(enabled));
        if enabled {
            assert_eq!(events[0]["level"], "WARN");
            assert_eq!(
                events[0]["fields"],
                serde_json::json!({
                    "event": "home_fs_roots_enabled",
                    "message": "Home and SSH filesystem roots explicitly enabled",
                })
            );
        }
        return;
    }

    let root = std::env::temp_dir().join(format!("chatos-home-roots-{}", uuid::Uuid::new_v4()));
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
    #[cfg(unix)]
    let layouts = [false, true];
    #[cfg(not(unix))]
    let layouts = [false];
    for linked in layouts {
        for overlap in ["none", "parent", "home", "ssh", "child"] {
            for host in ["primary", "legacy", "disabled"] {
                for (index, (value, opted_in)) in values.iter().enumerate() {
                    let root = fixture.0.join(format!("{linked}-{overlap}-{host}-{index}"));
                    let home = root.join("private/home");
                    for relative in ["", ".ssh", "project"] {
                        fs::create_dir_all(home.join(relative)).unwrap();
                        fs::write(home.join(relative).join("sentinel"), "unchanged").unwrap();
                    }
                    #[cfg(unix)]
                    if linked {
                        let target = root.join("external-ssh");
                        fs::rename(home.join(".ssh"), &target).unwrap();
                        std::os::unix::fs::symlink(target, home.join(".ssh")).unwrap();
                    }
                    let cwd = root.join("cwd");
                    fs::create_dir(&cwd).unwrap();
                    let module = module_path!().split_once("::").unwrap().1;
                    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                    command
                        .args([
                            "--exact",
                            &format!("{module}::home_roots_require_separate_opt_in"),
                            "--nocapture",
                        ])
                        .current_dir(cwd)
                        .env(CHILD, &root)
                        .env(ENABLED, (host != "disabled" && *opted_in).to_string())
                        .env(OVERLAP, overlap)
                        .env("CHATOS_TEST_HOME_LINKED", linked.to_string())
                        .env("CHATOS_TEST_HOME_HOST", host)
                        .env("CHATOS_WORKSPACE_DIR", root.join("workspace"))
                        .env("HOME", &home)
                        .env("USERPROFILE", &home)
                        .env_remove("HOMEDRIVE")
                        .env_remove("HOMEPATH")
                        .env_remove("CHATOS_ENABLE_HOST_FS_ROOTS")
                        .env_remove("FS_ENABLE_HOST_ROOTS")
                        .env_remove("CHATOS_ENABLE_REPO_PARENT_FS_ROOT")
                        .env_remove("CHATOS_ENABLE_HOME_FS_ROOTS")
                        .env_remove("FS_ALLOWED_ROOTS");
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
                    let configured = match overlap {
                        "parent" => Some(root.join("private")),
                        "home" => Some(home.clone()),
                        "ssh" => Some(home.join(".ssh")),
                        "child" => Some(home.join("project")),
                        _ => None,
                    };
                    if let Some(configured) = configured {
                        command.env("FS_ALLOWED_ROOTS", configured);
                    }
                    if let Some(value) = value {
                        command.env("CHATOS_ENABLE_HOME_FS_ROOTS", value);
                    }
                    let output = command.output().unwrap();
                    assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
                    assert!(
                        output.status.success(),
                        "linked={linked}, overlap={overlap}, host={host}, value={value:?}:\n{}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
    }
}

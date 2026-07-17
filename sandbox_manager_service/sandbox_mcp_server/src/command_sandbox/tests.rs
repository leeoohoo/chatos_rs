// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::config::*;
use super::permissions::*;

use super::*;
use chatos_sandbox_contract::{
    AdditionalFileSystemPermissions, AdditionalNetworkPermissions, NetworkDomainPermission,
    NetworkProxyMode, NetworkRequirements, RequestPermissionProfile,
};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType, PKCS_ECDSA_P256_SHA256,
};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig as TlsServerConfig;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

fn config(root: &Path, profile: PermissionProfileId) -> CommandSandboxConfig {
    let workspace = root.join("workspace");
    let state_root = root.join("state");
    let home = state_root.join("home");
    let temp = state_root.join("tmp");
    for path in [&workspace, &home, &temp] {
        std::fs::create_dir_all(path).expect("create path");
    }
    std::fs::create_dir_all(workspace.join(".git")).expect("git");
    let workspace = workspace.canonicalize().expect("workspace");
    CommandSandboxConfig {
        backend: CommandSandboxBackend::Native,
        workspace: workspace.clone(),
        state_root: state_root.canonicalize().expect("state"),
        temp: temp.canonicalize().expect("temp"),
        host_home: std::env::var_os("HOME").map(PathBuf::from),
        permission_profile: profile,
        runtime_workspace_roots: vec![workspace],
        base_file_system: legacy_file_system_policy(profile, &[]),
        network_unrestricted: profile == PermissionProfileId::FullAccess,
        network_proxy: None,
    }
}

#[test]
fn root_permission_materializes_absolute_platform_roots() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-root-permission-test-{}",
        uuid::Uuid::new_v4()
    ));
    let config = config(root.as_path(), PermissionProfileId::ReadOnly);
    let materialized = materialize_permissions(&config, config.workspace.as_path(), None)
        .expect("materialize root permission");
    let roots = filesystem_root_paths(&config, config.workspace.as_path());

    assert!(!roots.is_empty());
    assert!(roots.iter().all(|path| path.is_absolute()));
    assert!(roots.iter().all(|root| materialized
        .entries
        .iter()
        .any(|entry| { entry.path == *root && entry.access == FileSystemAccessMode::Read })));
    assert!(materialized.full_disk_read);
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn permission_paths_preserve_symlinks_until_backend_materialization() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "chatos-command-symlink-normalization-test-{}",
        uuid::Uuid::new_v4()
    ));
    let real = root.join("real");
    let link = root.join("link");
    std::fs::create_dir_all(real.as_path()).expect("real root");
    symlink(real.as_path(), link.as_path()).expect("symlink root");

    let logical = link.join("missing/child");
    assert_eq!(
        normalize_policy_path_preserving_symlinks(logical.as_path())
            .expect("normalize logical path"),
        logical
    );
    assert_eq!(
        canonicalize_preserving_missing(logical.as_path()).expect("canonicalize target"),
        real.canonicalize()
            .expect("canonical real root")
            .join("missing/child")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn symlinked_writable_roots_use_real_targets_and_nested_symlinks_fail_closed() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "chatos-command-symlink-policy-test-{}",
        uuid::Uuid::new_v4()
    ));
    let real = root.join("real");
    let link = root.join("link");
    let outside = root.join("outside");
    std::fs::create_dir_all(real.join("readonly")).expect("real readonly");
    std::fs::create_dir_all(outside.as_path()).expect("outside");
    symlink(real.as_path(), link.as_path()).expect("symlink root");
    symlink(outside.as_path(), real.join("escape")).expect("nested symlink");
    let real_target = real.canonicalize().expect("canonical real root");

    let materialized = MaterializedPermissions {
        unrestricted: false,
        full_disk_read: false,
        include_platform_defaults: false,
        entries: vec![
            MaterializedEntry {
                access: FileSystemAccessMode::Write,
                path: link.clone(),
            },
            MaterializedEntry {
                access: FileSystemAccessMode::Read,
                path: link.join("readonly"),
            },
        ],
    };
    let writable_roots = materialized_writable_roots(&materialized);
    assert_eq!(
        writable_roots,
        vec![MaterializedWritableRoot {
            logical: link.clone(),
            mount: real_target.clone(),
        }]
    );
    assert_eq!(
        remap_path_for_writable_root(link.join("readonly").as_path(), writable_roots.as_slice()),
        real_target.join("readonly")
    );

    let allowed = allowed_write_paths(writable_roots.as_slice());
    let escaped = remap_path_for_writable_root(
        link.join("escape/secret.txt").as_path(),
        writable_roots.as_slice(),
    );
    assert_eq!(
        first_writable_symlink_component_in_path(escaped.as_path(), allowed.as_slice()),
        Some(real_target.join("escape"))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn deny_glob_expansion_keeps_logical_matches_and_real_targets() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "chatos-command-symlink-glob-test-{}",
        uuid::Uuid::new_v4()
    ));
    let real = root.join("real");
    let link = root.join("link");
    std::fs::create_dir_all(real.join("nested")).expect("real nested");
    std::fs::write(real.join("nested/secret.env"), "secret").expect("secret file");
    symlink(real.as_path(), link.as_path()).expect("symlink root");

    let matches = expand_deny_glob(
        format!("{}/**/*.env", link.display()).as_str(),
        root.as_path(),
        Some(4),
    )
    .expect("expand deny glob");
    assert!(
        matches.contains(&link.join("nested/secret.env")),
        "{matches:?}"
    );
    assert!(
        matches.contains(
            &real
                .canonicalize()
                .expect("canonical real root")
                .join("nested/secret.env")
        ),
        "{matches:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn native_workspace_metadata_symlink_is_rejected_fail_closed() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "chatos-command-seatbelt-symlink-test-{}",
        uuid::Uuid::new_v4()
    ));
    let config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    let target = root.join("git-target");
    std::fs::create_dir_all(target.as_path()).expect("git target");
    std::fs::remove_dir_all(config.workspace.join(".git")).expect("remove ordinary git dir");
    symlink(target.as_path(), config.workspace.join(".git")).expect("symlink git dir");

    let error = PreparedSandboxCommand::new(
        &config,
        "/bin/sh",
        "true",
        config.workspace.as_path(),
        &TerminalCommandPermissions::default(),
    )
    .err()
    .expect("writable metadata symlink should fail closed");
    assert!(error.contains("crosses writable symlink"), "{error}");
    let _ = std::fs::remove_dir_all(root);
}

async fn run(
    config: &CommandSandboxConfig,
    command: String,
    permissions: TerminalCommandPermissions,
) -> std::process::Output {
    let mut prepared = PreparedSandboxCommand::new(
        config,
        "/bin/sh",
        command.as_str(),
        config.workspace.as_path(),
        &permissions,
    )
    .expect("prepare command");
    prepared
        .command_mut()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let spawned = prepared.spawn().expect("spawn");
    let output = spawned.child.wait_with_output().await.expect("output");
    spawned.cleanup.run();
    output
}

#[tokio::test]
async fn workspace_policy_and_command_overlay_are_enforced_per_child() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-sandbox-test-{}",
        uuid::Uuid::new_v4()
    ));
    let config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).expect("outside");
    let base = run(
        &config,
        format!(
            "touch '{}' && ! touch '{}' && ! touch '{}'",
            config.workspace.join("inside").display(),
            outside.join("blocked").display(),
            config.workspace.join(".git/blocked").display()
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        base.status.success(),
        "{}",
        String::from_utf8_lossy(&base.stderr)
    );

    let requested = RequestPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Path {
                    path: outside.to_string_lossy().to_string(),
                },
            }]),
            ..Default::default()
        }),
        network: None,
    };
    let elevated = run(
        &config,
        format!("touch '{}'", outside.join("allowed").display()),
        TerminalCommandPermissions {
            requested: Some(requested.clone()),
            granted: Some(requested.into()),
        },
    )
    .await;
    assert!(
        elevated.status.success(),
        "{}",
        String::from_utf8_lossy(&elevated.stderr)
    );
    assert!(outside.join("allowed").exists());

    std::fs::write(outside.join("secret.env"), "secret").expect("secret file");
    let constrained_request = RequestPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: outside.to_string_lossy().to_string(),
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::GlobPattern {
                        pattern: format!("{}/**/*.env", outside.display()),
                    },
                },
            ]),
            glob_scan_max_depth: Some(3),
            ..Default::default()
        }),
        network: None,
    };
    let constrained = run(
        &config,
        format!(
            "touch '{}' && ! cat '{}' && ! rm '{}'",
            outside.join("ordinary.txt").display(),
            outside.join("secret.env").display(),
            outside.join("secret.env").display(),
        ),
        TerminalCommandPermissions {
            requested: Some(constrained_request.clone()),
            granted: Some(constrained_request.into()),
        },
    )
    .await;
    assert!(
        constrained.status.success(),
        "{}",
        String::from_utf8_lossy(&constrained.stderr)
    );
    assert!(outside.join("ordinary.txt").exists());
    assert!(outside.join("secret.env").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn symlinked_writable_root_binds_real_target_and_preserves_read_carveout() {
    use std::os::unix::fs::symlink;

    let root = std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("host home")
        .join(format!(
            ".chatos-command-symlinked-write-root-test-{}",
            uuid::Uuid::new_v4()
        ));
    let mut config = config(root.as_path(), PermissionProfileId::ReadOnly);
    let real = root.join("real-write-root");
    let link = root.join("linked-write-root");
    std::fs::create_dir_all(real.join("readonly")).expect("readonly root");
    symlink(real.as_path(), link.as_path()).expect("symlink writable root");
    config.workspace = real.canonicalize().expect("canonical real workspace");
    config.runtime_workspace_roots = vec![config.workspace.clone()];
    config.base_file_system = FileSystemPermissionPolicy::Restricted {
        entries: vec![
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root,
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Path {
                    path: link.to_string_lossy().to_string(),
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Path {
                    path: link.join("readonly").to_string_lossy().to_string(),
                },
            },
        ],
        glob_scan_max_depth: None,
    };

    let output = run(
        &config,
        format!(
            "touch '{}' && ! touch '{}'",
            link.join("created.txt").display(),
            link.join("readonly/blocked.txt").display(),
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(real.join("created.txt").exists());
    assert!(!real.join("readonly/blocked.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn custom_profile_enforces_multi_root_carveouts_and_deny_globs() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-custom-profile-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    let shared = root.join("shared");
    std::fs::create_dir_all(shared.join("readonly")).expect("shared readonly");
    std::fs::create_dir_all(config.workspace.join("readonly")).expect("workspace readonly");
    std::fs::write(config.workspace.join("workspace.env"), "secret").expect("workspace env");
    std::fs::write(shared.join("shared.env"), "secret").expect("shared env");
    let shared = shared.canonicalize().expect("shared");
    config.runtime_workspace_roots = vec![config.workspace.clone(), shared.clone()];
    config.base_file_system = FileSystemPermissionPolicy::Restricted {
        entries: vec![
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root,
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots {
                        subpath: Some("readonly".to_string()),
                    },
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Deny,
                path: FileSystemPath::GlobPattern {
                    pattern: "**/*.env".to_string(),
                },
            },
        ],
        glob_scan_max_depth: Some(4),
    };

    let output = run(
        &config,
        format!(
            "touch '{}' && touch '{}' && ! touch '{}' && ! touch '{}' && ! cat '{}' && ! cat '{}'",
            config.workspace.join("workspace.txt").display(),
            shared.join("shared.txt").display(),
            config.workspace.join("readonly/blocked.txt").display(),
            shared.join("readonly/blocked.txt").display(),
            config.workspace.join("workspace.env").display(),
            shared.join("shared.env").display(),
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(config.workspace.join("workspace.txt").exists());
    assert!(shared.join("shared.txt").exists());
    assert!(!config.workspace.join("readonly/blocked.txt").exists());
    assert!(!shared.join("readonly/blocked.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn restricted_minimal_profile_runs_tools_without_reading_unapproved_user_paths() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-minimal-profile-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    config.temp = std::env::temp_dir()
        .canonicalize()
        .expect("canonical system temp");
    let outside = config
        .host_home
        .as_deref()
        .expect("host home")
        .join(format!(
            ".chatos-command-minimal-outside-{}",
            uuid::Uuid::new_v4()
        ));
    std::fs::create_dir_all(outside.as_path()).expect("outside");
    std::fs::write(outside.join("secret.txt"), "secret").expect("outside secret");
    config.base_file_system = FileSystemPermissionPolicy::Restricted {
        entries: vec![
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Minimal,
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots {
                        subpath: Some(".git".to_string()),
                    },
                },
            },
        ],
        glob_scan_max_depth: None,
    };

    let materialized = materialize_permissions(&config, config.workspace.as_path(), None)
        .expect("materialize minimal profile");
    assert!(!materialized.full_disk_read);
    assert!(materialized.include_platform_defaults);

    let output = run(
        &config,
        format!(
            "echo \"TMPDIR=$TMPDIR\" && test -x /bin/sh && touch '{}' && test -f \"$(mktemp)\" && ! cat '{}'",
            config.workspace.join("created.txt").display(),
            outside.join("secret.txt").display(),
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(config.workspace.join("created.txt").exists());
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn restricted_read_workspace_below_private_tmp_shadow_remains_visible() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-restricted-read-tmp-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::ReadOnly);
    std::fs::write(config.workspace.join("readable.txt"), "visible").expect("workspace file");
    config.base_file_system = FileSystemPermissionPolicy::Restricted {
        entries: vec![
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Minimal,
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                },
            },
        ],
        glob_scan_max_depth: None,
    };

    let output = run(
        &config,
        format!(
            "test \"$(cat '{}')\" = visible && ! touch '{}'",
            config.workspace.join("readable.txt").display(),
            config.workspace.join("blocked.txt").display(),
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!config.workspace.join("blocked.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn command_write_overlay_cannot_override_a_base_deny() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-base-deny-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::ReadOnly);
    let secret = config.workspace.join("secret.txt");
    std::fs::write(secret.as_path(), "secret").expect("secret");
    config.base_file_system = FileSystemPermissionPolicy::Restricted {
        entries: vec![
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root,
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Deny,
                path: FileSystemPath::Path {
                    path: secret.to_string_lossy().to_string(),
                },
            },
        ],
        glob_scan_max_depth: None,
    };
    let requested = RequestPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: config.workspace.to_string_lossy().to_string(),
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: secret.to_string_lossy().to_string(),
                    },
                },
            ]),
            ..Default::default()
        }),
        network: None,
    };

    let output = run(
        &config,
        format!(
            "touch '{}' && ! cat '{}' && ! rm '{}'",
            config.workspace.join("ordinary.txt").display(),
            secret.display(),
            secret.display(),
        ),
        TerminalCommandPermissions {
            requested: Some(requested.clone()),
            granted: Some(requested.into()),
        },
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(config.workspace.join("ordinary.txt").exists());
    assert!(secret.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_tool_policy_does_not_infer_workspace_write_from_an_external_write_root() {
    let root = std::env::temp_dir().join(format!(
        "chatos-file-tool-external-write-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).expect("outside");
    config.base_file_system = FileSystemPermissionPolicy::Restricted {
        entries: vec![
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root,
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Path {
                    path: outside.to_string_lossy().to_string(),
                },
            },
        ],
        glob_scan_max_depth: None,
    };

    let policy = config.file_tool_access_policy().expect("file tool policy");
    assert!(!policy.workspace_writes_allowed());
    assert!(policy
        .authorize_write(config.workspace.join("blocked.txt").as_path())
        .is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_tool_policy_enforces_read_carveouts_and_deny_globs() {
    let root = std::env::temp_dir().join(format!(
        "chatos-file-tool-carveout-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    let readonly = config.workspace.join("readonly");
    let secret = config.workspace.join("secret.env");
    std::fs::create_dir_all(&readonly).expect("readonly");
    std::fs::write(&secret, "secret").expect("secret");
    config.base_file_system = FileSystemPermissionPolicy::Restricted {
        entries: vec![
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root,
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Path {
                    path: readonly.to_string_lossy().to_string(),
                },
            },
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Deny,
                path: FileSystemPath::GlobPattern {
                    pattern: "**/*.env".to_string(),
                },
            },
        ],
        glob_scan_max_depth: Some(3),
    };

    let policy = config.file_tool_access_policy().expect("file tool policy");
    assert!(policy.workspace_writes_allowed());
    assert!(policy
        .authorize_write(config.workspace.join("ordinary.txt").as_path())
        .is_ok());
    assert!(policy
        .authorize_write(readonly.join("blocked.txt").as_path())
        .is_err());
    assert!(policy.authorize_read(secret.as_path()).is_err());
    assert!(policy
        .authorize_recursive_read(config.workspace.as_path())
        .is_err());
    assert!(policy
        .authorize_recursive_write(config.workspace.as_path())
        .is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn restricted_network_uses_proxy_and_blocks_direct_bypass() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-network-proxy-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("upstream listener");
    let upstream_port = listener.local_addr().expect("upstream address").port();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept upstream");
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.expect("read request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nproxied")
            .await
            .expect("write response");
    });
    config.network_proxy = NetworkProxyRuntime::start(
        config.state_root.as_path(),
        &NetworkRequirements {
            enabled: Some(true),
            domains: Some(BTreeMap::from([(
                "127.0.0.1".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("start network proxy");

    let proxied = run(
        &config,
        format!(
            "/usr/bin/curl --silent --show-error --max-time 3 http://127.0.0.1:{upstream_port}/"
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        proxied.status.success(),
        "{}",
        String::from_utf8_lossy(&proxied.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&proxied.stdout), "proxied");

    let bypass = run(
        &config,
        format!(
            "/usr/bin/curl --noproxy '*' --silent --show-error --max-time 1 http://127.0.0.1:{upstream_port}/"
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(!bypass.status.success(), "direct network bypass succeeded");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn restricted_https_uses_managed_ca_and_enforces_inner_method_policy() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let root = std::env::temp_dir().join(format!(
        "chatos-command-https-proxy-test-{}",
        uuid::Uuid::new_v4()
    ));
    let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);

    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "ChatOS command sandbox HTTPS test CA");
    ca_params.distinguished_name = distinguished_name;
    let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("CA key");
    let ca_certificate = ca_params.self_signed(&ca_key).expect("CA certificate");
    let ca_path = config.state_root.join("upstream-test-ca.pem");
    std::fs::write(ca_path.as_path(), ca_certificate.pem()).expect("write test CA");
    let issuer = Issuer::new(ca_params, ca_key);

    let mut leaf_params = CertificateParams::new(Vec::new()).expect("leaf params");
    leaf_params
        .subject_alt_names
        .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("leaf key");
    let leaf_certificate = leaf_params
        .signed_by(&leaf_key, &issuer)
        .expect("leaf certificate");
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let mut tls_config = TlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![leaf_certificate.der().clone()], private_key)
        .expect("TLS server config");
    tls_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("HTTPS upstream listener");
    let upstream_port = listener.local_addr().expect("upstream address").port();
    let upstream_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept HTTPS GET");
        let mut stream = acceptor.accept(stream).await.expect("accept upstream TLS");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).await.expect("read HTTPS GET");
        assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nhttps-proxied",
            )
            .await
            .expect("write HTTPS response");
        stream.shutdown().await.expect("shutdown upstream TLS");

        let (stream, _) = listener.accept().await.expect("accept HTTPS POST route");
        assert!(
            acceptor.accept(stream).await.is_err(),
            "blocked POST must not start upstream TLS"
        );
    });

    let previous_ssl_cert_file = std::env::var_os("SSL_CERT_FILE");
    std::env::set_var("SSL_CERT_FILE", ca_path.as_os_str());
    config.network_proxy = NetworkProxyRuntime::start(
        config.state_root.as_path(),
        &NetworkRequirements {
            enabled: Some(true),
            mode: Some(NetworkProxyMode::Limited),
            domains: Some(BTreeMap::from([(
                "127.0.0.1".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("start HTTPS network proxy");
    if let Some(value) = previous_ssl_cert_file {
        std::env::set_var("SSL_CERT_FILE", value);
    } else {
        std::env::remove_var("SSL_CERT_FILE");
    }

    let get = run(
        &config,
        format!(
            "/usr/bin/curl --fail --silent --show-error --max-time 5 https://127.0.0.1:{upstream_port}/"
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        get.status.success(),
        "{}",
        String::from_utf8_lossy(&get.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&get.stdout), "https-proxied");

    let post = run(
        &config,
        format!(
            "/usr/bin/curl --fail --silent --show-error --max-time 5 --request POST --data '' https://127.0.0.1:{upstream_port}/"
        ),
        TerminalCommandPermissions::default(),
    )
    .await;
    assert!(
        !post.status.success(),
        "limited HTTPS POST unexpectedly succeeded"
    );
    assert!(
        String::from_utf8_lossy(&post.stderr).contains("403"),
        "{}",
        String::from_utf8_lossy(&post.stderr)
    );
    upstream_task.await.expect("upstream task");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn untrusted_grant_without_request_is_rejected() {
    let root = std::env::temp_dir().join(format!(
        "chatos-command-sandbox-grant-test-{}",
        uuid::Uuid::new_v4()
    ));
    let config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
    let err = match PreparedSandboxCommand::new(
        &config,
        "/bin/sh",
        "true",
        config.workspace.as_path(),
        &TerminalCommandPermissions {
            requested: None,
            granted: Some(GrantedPermissionProfile {
                file_system: None,
                network: Some(AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
            }),
        },
    ) {
        Ok(_) => panic!("grant injection must fail"),
        Err(err) => err,
    };
    assert!(err.contains("no matching request"));
    let _ = std::fs::remove_dir_all(root);
}

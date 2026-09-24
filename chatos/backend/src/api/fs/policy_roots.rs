// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::core::auth::AuthUser;
use crate::utils::workspace::resolve_workspace_dir;

use super::super::roots::home_dir;
use super::policy_paths::{canonicalize_existing_dir, normalize_path_for_compare};
use super::{FsAllowedRoot, FsAllowedRootKind};

pub(super) async fn build_allowed_roots(auth: &AuthUser) -> Vec<FsAllowedRoot> {
    let mut roots = Vec::new();
    let host_roots_enabled = host_fs_roots_enabled();
    let user_roots = ensure_user_scoped_roots(auth);

    if let Some(user_roots) = user_roots.as_ref() {
        push_user_scoped_roots(&mut roots, user_roots);
    }

    if host_roots_enabled {
        if let Ok(current_dir) = env::current_dir() {
            push_root(&mut roots, current_dir, FsAllowedRootKind::CurrentDir);
        }

        if let Ok(current_dir) = env::current_dir() {
            let repo_root = current_dir
                .ancestors()
                .find(|candidate| candidate.join(".git").exists())
                .unwrap_or(current_dir.as_path());
            if let Some(parent) = repo_root.parent() {
                push_root(
                    &mut roots,
                    parent.to_path_buf(),
                    FsAllowedRootKind::RepoParent,
                );
            }
        }

        push_root(
            &mut roots,
            PathBuf::from(resolve_workspace_dir(None)),
            FsAllowedRootKind::Workspace,
        );

        if let Some(home) = home_dir() {
            push_root(&mut roots, home.join(".ssh"), FsAllowedRootKind::Ssh);
            push_root(&mut roots, home, FsAllowedRootKind::Home);
        }

        if let Ok(raw) = env::var("FS_ALLOWED_ROOTS") {
            for value in raw
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                push_root(
                    &mut roots,
                    PathBuf::from(value),
                    FsAllowedRootKind::Configured,
                );
            }
        }
    }

    roots.sort_by(|left, right| {
        left.kind
            .priority()
            .cmp(&right.kind.priority())
            .then_with(|| left.path.cmp(&right.path))
    });

    roots
}

#[derive(Debug, Clone)]
struct UserScopedRoots {
    workspaces_root: PathBuf,
    public_root: PathBuf,
}

fn push_user_scoped_roots(roots: &mut Vec<FsAllowedRoot>, user_roots: &UserScopedRoots) {
    for (expected, kind) in [
        (&user_roots.workspaces_root, FsAllowedRootKind::Workspace),
        (&user_roots.public_root, FsAllowedRootKind::Public),
    ] {
        let Ok(canonical) = canonicalize_existing_dir(expected) else {
            continue;
        };
        // These paths were already canonical when the user directories were
        // validated. A later redirect must never become a new authorization root.
        if canonical != *expected {
            continue;
        }
        // Register this checked value without resolving the path a second time.
        push_canonical_root(roots, canonical, kind);
    }
}

fn ensure_user_scoped_roots(auth: &AuthUser) -> Option<UserScopedRoots> {
    let base = PathBuf::from(resolve_workspace_dir(None));
    // The configured base may be a deployment-managed symlink. Below that
    // boundary every component must be a real directory, never a redirect to
    // another user's directory or a host path.
    fs::create_dir_all(&base).ok()?;
    let base = canonicalize_existing_dir(&base).ok()?;
    let users_root = ensure_child_directory(&base, "users").ok()?;
    let user_component = user_path_component(auth.user_id.as_str());
    let user_root = ensure_child_directory(&users_root, &user_component).ok()?;
    let workspaces_root = ensure_child_directory(&user_root, "workspaces").ok()?;
    let public_root = ensure_child_directory(&user_root, "public").ok()?;
    set_private_dir_permissions(user_root.as_path()).ok()?;
    set_private_dir_permissions(workspaces_root.as_path()).ok()?;
    set_private_dir_permissions(public_root.as_path()).ok()?;
    Some(UserScopedRoots {
        workspaces_root,
        public_root,
    })
}

fn ensure_child_directory(parent: &Path, name: &str) -> std::io::Result<PathBuf> {
    // Descriptor-relative creation must receive exactly one child name.
    if !matches!(Path::new(name).components().next(), Some(std::path::Component::Normal(component)) if component == name)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected a single normal directory component",
        ));
    }
    let path = parent.join(name);
    #[cfg(unix)]
    let created = {
        use std::ffi::CString;
        use std::os::fd::AsRawFd;

        // Reject redirected ancestors before any mutation, and keep the parent
        // open so replacement after the walk cannot redirect mkdir.
        let directory = open_directory_without_symlinks(parent)?;
        let name = CString::new(name)?;
        // SAFETY: directory owns a live descriptor and name is a NUL-terminated
        // single child component. Private mode applies at creation, before chmod.
        let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
        if result == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    };
    #[cfg(not(unix))]
    let created = fs::create_dir(&path);
    match created {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err),
    }
    // Do not follow existing symlinks, including dangling ones. Check before
    // creating descendants or changing permissions on an existing directory.
    if !fs::symlink_metadata(&path)?.file_type().is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "user root component is not a real directory",
        ));
    }
    let canonical = canonicalize_existing_dir(&path)?;
    if normalize_path_for_compare(&canonical) != normalize_path_for_compare(&path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "user root component redirects outside its expected path",
        ));
    }
    Ok(canonical)
}

fn set_private_dir_permissions(_path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let directory = open_directory_without_symlinks(_path)?;
        directory.set_permissions(fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(unix)]
fn open_directory_without_symlinks(path: &Path) -> std::io::Result<fs::File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Component;

    let mut components = path.components();
    if components.next() != Some(Component::RootDir) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected an absolute canonical directory path",
        ));
    }
    let flags = libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC;
    let mut directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open("/")?;
    for component in components {
        let Component::Normal(name) = component else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "expected only normal directory components",
            ));
        };
        let name = CString::new(name.as_bytes())?;
        // Walk one component relative to a held directory descriptor. A renamed
        // or replaced ancestor cannot redirect subsequent opens or the chmod.
        // SAFETY: directory owns a live descriptor; name is a NUL-terminated
        // single component. O_CREAT is absent, so no mode argument is required.
        let fd =
            unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags | libc::O_RDONLY) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: successful openat returns a new descriptor owned only here.
        directory = unsafe { fs::File::from_raw_fd(fd) };
    }
    Ok(directory)
}

pub(crate) fn user_path_component(user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_id.as_bytes());
    let digest = hex::encode(hasher.finalize());
    let suffix = &digest[..16];
    let prefix = safe_path_component(user_id);
    format!("{prefix}-{suffix}")
}

fn safe_path_component(value: &str) -> String {
    let mut out = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while out.starts_with('.') {
        out.remove(0);
    }
    if out.is_empty() {
        "user".to_string()
    } else {
        out
    }
}

pub(crate) fn log_host_fs_roots_configuration() {
    if host_fs_roots_enabled() {
        tracing::warn!(
            event = "host_fs_roots_enabled",
            "Host filesystem roots explicitly enabled"
        );
    }
}

fn host_fs_roots_enabled() -> bool {
    env_bool_override("CHATOS_ENABLE_HOST_FS_ROOTS")
        .or_else(|| env_bool_override("FS_ENABLE_HOST_ROOTS"))
        .unwrap_or(false)
}

fn env_bool_override(key: &str) -> Option<bool> {
    match env::var(key) {
        Ok(value) => Some(matches_env_bool(value.trim())),
        Err(env::VarError::NotPresent) => None,
        // A present but malformed primary switch must not fall back to an
        // enabled legacy alias. Only absence permits that fallback.
        Err(env::VarError::NotUnicode(_)) => Some(false),
    }
}

fn matches_env_bool(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn push_root(roots: &mut Vec<FsAllowedRoot>, candidate: PathBuf, kind: FsAllowedRootKind) {
    let Ok(canonical) = canonicalize_existing_dir(candidate.as_path()) else {
        return;
    };
    push_canonical_root(roots, canonical, kind);
}

fn push_canonical_root(
    roots: &mut Vec<FsAllowedRoot>,
    canonical: PathBuf,
    kind: FsAllowedRootKind,
) {
    let normalized = normalize_path_for_compare(canonical.as_path());
    if let Some(root) = roots.iter_mut().find(|root| {
        // Unix directory identity must retain native components. Compatibility
        // normalization can alias distinct roots and discard their restrictions.
        (!cfg!(unix) || root.path == canonical)
            && normalize_path_for_compare(root.path.as_path()) == normalized
    }) {
        // Preserve navigation identity, but never discard a read-only restriction
        // when the same canonical root is discovered through another source.
        root.can_write &= kind.can_write();
        return;
    }
    roots.push(FsAllowedRoot {
        path: canonical,
        kind,
        can_write: kind.can_write(),
    });
}

#[cfg(test)]
#[path = "policy_roots_isolation_tests.rs"]
mod isolation_tests;

#[cfg(test)]
#[path = "policy_roots_permissions_tests.rs"]
mod permissions_tests;

#[cfg(test)]
mod tests {
    use super::{host_fs_roots_enabled, log_host_fs_roots_configuration, user_path_component};
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct LogCapture(Arc<Mutex<Vec<u8>>>);

    impl Write for LogCapture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn user_path_component_avoids_sanitization_collisions() {
        assert_ne!(user_path_component("a/b"), user_path_component("a_b"));
    }

    #[test]
    fn user_path_component_keeps_readable_prefix() {
        let value = user_path_component(" user-1 ");
        assert!(value.starts_with("user-1-"));
        assert!(value.len() > "user-1-".len());
    }

    #[test]
    fn host_fs_roots_require_explicit_opt_in() {
        const EXPECTED: &str = "CHATOS_TEST_HOST_FS_ROOTS_EXPECTED";
        if let Ok(expected) = std::env::var(EXPECTED) {
            let enabled = expected == "true";
            assert_eq!(host_fs_roots_enabled(), enabled);
            let capture = LogCapture::default();
            let writer = capture.clone();
            let subscriber = tracing_subscriber::fmt()
                .without_time()
                .with_ansi(false)
                .with_target(false)
                .json()
                .with_writer(move || writer.clone())
                .finish();
            tracing::subscriber::with_default(subscriber, || {
                log_host_fs_roots_configuration();
                // Request-time policy checks must not repeat the startup event.
                assert_eq!(host_fs_roots_enabled(), enabled);
                assert_eq!(host_fs_roots_enabled(), enabled);
            });
            let bytes = capture.0.lock().unwrap().clone();
            let logs = String::from_utf8(bytes).unwrap();
            if enabled {
                let events = logs.lines().collect::<Vec<_>>();
                assert_eq!(events.len(), 1, "missing or repeated startup audit event");
                let event: serde_json::Value = serde_json::from_str(events[0]).unwrap();
                assert_eq!(event["level"], "WARN");
                assert_eq!(
                    event["fields"],
                    serde_json::json!({
                        "message": "Host filesystem roots explicitly enabled",
                        "event": "host_fs_roots_enabled",
                    }),
                    "audit fields must contain no paths or raw configuration values",
                );
            } else {
                assert!(
                    logs.is_empty(),
                    "disabled host roots must not report enablement"
                );
            }
            return;
        }

        // Isolate environment variables in child processes so parallel tests never
        // observe a temporary host-filesystem permission change.
        for node_env in [
            None,
            Some(""),
            Some("development"),
            Some("test"),
            Some("production"),
            Some("PRODUCTION"),
            Some(" production "),
            Some("staging"),
            Some("prodution"),
        ] {
            let cases = [
                (None, None, false),
                (Some("true"), None, true),
                (None, Some("true"), true),
                (Some("false"), Some("true"), false),
                (Some("true"), Some("false"), true),
                (Some("false"), Some("false"), false),
                (Some(""), Some("true"), false),
                (Some("invalid"), Some("true"), false),
                (None, Some("invalid"), false),
                (Some(" TRUE "), None, true),
                (Some("1"), None, true),
                (None, Some("on"), true),
                (None, Some("yes"), true),
                (None, Some("0"), false),
            ]
            .map(|(primary, legacy, expected)| {
                (
                    primary.map(std::ffi::OsStr::new),
                    legacy.map(std::ffi::OsStr::new),
                    expected,
                )
            });
            #[cfg(unix)]
            let cases = {
                use std::ffi::OsStr;
                use std::os::unix::ffi::OsStrExt;

                let invalid = OsStr::from_bytes(b"\xfftrue");
                [
                    &cases[..],
                    &[
                        (Some(invalid), Some(OsStr::new("true")), false),
                        (Some(invalid), None, false),
                        (None, Some(invalid), false),
                        (Some(OsStr::new("true")), Some(invalid), true),
                        (Some(OsStr::new("false")), Some(invalid), false),
                    ],
                ]
                .concat()
            };
            for (primary, legacy, expected) in cases {
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                let module = module_path!().split_once("::").unwrap().1;
                command
                    .arg("--exact")
                    .arg(format!("{module}::host_fs_roots_require_explicit_opt_in"))
                    .arg("--nocapture")
                    .env(EXPECTED, expected.to_string());
                for (key, value) in [
                    ("NODE_ENV", node_env.map(std::ffi::OsStr::new)),
                    ("CHATOS_ENABLE_HOST_FS_ROOTS", primary),
                    ("FS_ENABLE_HOST_ROOTS", legacy),
                ] {
                    command.env_remove(key);
                    if let Some(value) = value {
                        command.env(key, value);
                    }
                }
                let output = command.output().unwrap();
                assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
                assert!(
                    output.status.success(),
                    "NODE_ENV={node_env:?}, primary={primary:?}, legacy={legacy:?}:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                );
            }
        }
    }
}

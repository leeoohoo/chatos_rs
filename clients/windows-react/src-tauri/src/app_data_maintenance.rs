use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

use crate::app_owned_export::try_remove_stale_owned_file;

const STALE_APP_DATA_TEMPORARY_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAXIMUM_ENTRIES_PER_SCOPE: usize = 2_000;
const MAXIMUM_ENTRIES_PER_TEMPORARY_TREE: usize = 20_000;

const PREFERENCE_PREFIXES: &[&str] =
    &[".general-settings-", ".global-utilities-", ".quick-search-"];
const PET_PREFIXES: &[&str] = &[".preferences-", ".project-favorites-"];
const PROJECT_RUN_PREFIXES: &[&str] = &[".preferences-"];
const REMOTE_CONNECTION_PREFIXES: &[&str] = &[".credential-index-"];
const CLIPBOARD_PREFIXES: &[&str] = &[".clipboard-"];
const APPROVAL_PREFIXES: &[&str] = &[".approval-"];
const INVOCATION_JOURNAL_PREFIXES: &[&str] = &["."];
const CONNECTOR_PREFIXES: &[&str] = &[".connector-state-", ".approval-history-"];

const CLEANUP_SCOPES: &[CleanupScope] = &[
    CleanupScope {
        components: &["preferences"],
        pattern: CleanupNamePattern::Uuid(PREFERENCE_PREFIXES),
    },
    CleanupScope {
        components: &["pet"],
        pattern: CleanupNamePattern::Uuid(PET_PREFIXES),
    },
    CleanupScope {
        components: &["project-run"],
        pattern: CleanupNamePattern::Uuid(PROJECT_RUN_PREFIXES),
    },
    CleanupScope {
        components: &["remote-connections"],
        pattern: CleanupNamePattern::Uuid(REMOTE_CONNECTION_PREFIXES),
    },
    CleanupScope {
        components: &["clipboard-history", "Payloads"],
        pattern: CleanupNamePattern::Uuid(CLIPBOARD_PREFIXES),
    },
    CleanupScope {
        components: &["local-agent", "approval-decisions"],
        pattern: CleanupNamePattern::Uuid(APPROVAL_PREFIXES),
    },
    CleanupScope {
        components: &["NativeConnector", "PluginRuntime", "InvocationJournal"],
        pattern: CleanupNamePattern::Uuid(INVOCATION_JOURNAL_PREFIXES),
    },
    CleanupScope {
        components: &["NativeConnector", "Plugins", ".integrity"],
        pattern: CleanupNamePattern::PluginIntegrity,
    },
    CleanupScope {
        components: &["NativeConnector"],
        pattern: CleanupNamePattern::Uuid(CONNECTOR_PREFIXES),
    },
];

const DIRECTORY_CLEANUP_SCOPES: &[DirectoryCleanupScope] = &[
    DirectoryCleanupScope {
        components: &["NativeConnector", "Plugins", ".source-staging"],
    },
    DirectoryCleanupScope {
        components: &["NativeConnector", "Plugins", ".staging"],
    },
];

struct CleanupScope {
    components: &'static [&'static str],
    pattern: CleanupNamePattern,
}

struct DirectoryCleanupScope {
    components: &'static [&'static str],
}

enum CleanupNamePattern {
    Uuid(&'static [&'static str]),
    PluginIntegrity,
}

pub(crate) async fn cleanup_stale_temporaries(root: PathBuf) -> usize {
    tauri::async_runtime::spawn_blocking(move || {
        let cutoff = SystemTime::now()
            .checked_sub(STALE_APP_DATA_TEMPORARY_AGE)
            .unwrap_or(UNIX_EPOCH);
        cleanup_stale_temporaries_before(&root, cutoff)
    })
    .await
    .unwrap_or_default()
}

fn cleanup_stale_temporaries_before(root: &Path, cutoff: SystemTime) -> usize {
    if !safe_directory(root) {
        return 0;
    }
    let files_removed = CLEANUP_SCOPES
        .iter()
        .map(|scope| cleanup_scope(root, scope, cutoff))
        .fold(0usize, usize::saturating_add);
    DIRECTORY_CLEANUP_SCOPES
        .iter()
        .map(|scope| cleanup_directory_scope(root, scope, cutoff))
        .fold(files_removed, usize::saturating_add)
}

fn cleanup_scope(root: &Path, scope: &CleanupScope, cutoff: SystemTime) -> usize {
    let mut directory = root.to_path_buf();
    for component in scope.components {
        directory.push(component);
        if !safe_directory(&directory) {
            return 0;
        }
    }
    let Ok(entries) = fs::read_dir(&directory) else {
        return 0;
    };
    let mut removed = 0usize;
    for (visited, entry) in entries.flatten().enumerate() {
        if visited >= MAXIMUM_ENTRIES_PER_SCOPE {
            break;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if canonical_temporary_name(&name, &scope.pattern)
            && try_remove_stale_owned_file(&entry.path(), cutoff)
        {
            removed = removed.saturating_add(1);
        }
    }
    removed
}

fn cleanup_directory_scope(
    root: &Path,
    scope: &DirectoryCleanupScope,
    cutoff: SystemTime,
) -> usize {
    let mut directory = root.to_path_buf();
    for component in scope.components {
        directory.push(component);
        if !safe_directory(&directory) {
            return 0;
        }
    }
    let Ok(entries) = fs::read_dir(&directory) else {
        return 0;
    };
    let mut removed = 0usize;
    for (visited, entry) in entries.enumerate() {
        if visited >= MAXIMUM_ENTRIES_PER_SCOPE {
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let path = entry.path();
        if canonical_uuid(&name)
            && stale_owned_temporary_tree(&path, cutoff)
            && fs::remove_dir_all(path).is_ok()
        {
            removed = removed.saturating_add(1);
        }
    }
    removed
}

fn stale_owned_temporary_tree(root: &Path, cutoff: SystemTime) -> bool {
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(path) = pending.pop() {
        visited = visited.saturating_add(1);
        if visited > MAXIMUM_ENTRIES_PER_TEMPORARY_TREE {
            return false;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return false;
        };
        if metadata.file_type().is_symlink()
            || is_windows_reparse_point(&metadata)
            || !metadata.modified().is_ok_and(|modified| modified <= cutoff)
        {
            return false;
        }
        if metadata.is_file() {
            if !single_link_regular_file(&path, &metadata) {
                return false;
            }
            continue;
        }
        if !metadata.is_dir() {
            return false;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return false;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return false;
            };
            pending.push(entry.path());
        }
    }
    true
}

#[cfg(windows)]
fn single_link_regular_file(path: &Path, _: &fs::Metadata) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        },
    };

    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let valid = unsafe { GetFileInformationByHandle(handle, &mut information) } != 0;
    let safe = valid
        && information.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
            == 0
        && information.nNumberOfLinks == 1;
    let _ = unsafe { CloseHandle(handle) };
    safe
}

#[cfg(unix)]
fn single_link_regular_file(_: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() == 1
}

#[cfg(not(any(windows, unix)))]
fn single_link_regular_file(_: &Path, _: &fs::Metadata) -> bool {
    false
}

fn canonical_temporary_name(name: &str, pattern: &CleanupNamePattern) -> bool {
    match pattern {
        CleanupNamePattern::Uuid(prefixes) => prefixes.iter().any(|prefix| {
            name.strip_prefix(prefix)
                .and_then(|value| value.strip_suffix(".tmp"))
                .is_some_and(canonical_uuid)
        }),
        CleanupNamePattern::PluginIntegrity => name
            .strip_prefix('.')
            .and_then(|value| value.strip_suffix(".tmp"))
            .and_then(|value| value.split_once('-'))
            .is_some_and(|(digest, token)| canonical_sha256(digest) && canonical_uuid(token)),
    }
}

fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value)
}

fn canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_dir()
            && !metadata.file_type().is_symlink()
            && !is_windows_reparse_point(&metadata)
    })
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_windows_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cleanup_is_limited_to_fixed_scopes_and_canonical_owned_names() {
        let root = tempdir().unwrap();
        let token = "00000000-0000-0000-0000-000000000000";
        let owned = [
            ("preferences", format!(".general-settings-{token}.tmp")),
            ("preferences", format!(".global-utilities-{token}.tmp")),
            ("preferences", format!(".quick-search-{token}.tmp")),
            ("pet", format!(".preferences-{token}.tmp")),
            ("pet", format!(".project-favorites-{token}.tmp")),
            ("project-run", format!(".preferences-{token}.tmp")),
            (
                "remote-connections",
                format!(".credential-index-{token}.tmp"),
            ),
            (
                "clipboard-history/Payloads",
                format!(".clipboard-{token}.tmp"),
            ),
            (
                "local-agent/approval-decisions",
                format!(".approval-{token}.tmp"),
            ),
            ("NativeConnector", format!(".connector-state-{token}.tmp")),
            ("NativeConnector", format!(".approval-history-{token}.tmp")),
        ]
        .into_iter()
        .map(|(directory, name)| {
            let directory = root.path().join(directory);
            fs::create_dir_all(&directory).unwrap();
            let path = directory.join(name);
            fs::write(&path, b"value").unwrap();
            path
        })
        .collect::<Vec<_>>();

        let unrelated = root
            .path()
            .join("preferences")
            .join(format!(".unknown-{token}.tmp"));
        let malformed = root
            .path()
            .join("preferences")
            .join(".general-settings-NOT-A-UUID.tmp");
        let outside = root
            .path()
            .join("outside")
            .join(format!(".general-settings-{token}.tmp"));
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        for path in [&unrelated, &malformed, &outside] {
            fs::write(path, b"value").unwrap();
        }

        assert_eq!(cleanup_stale_temporaries_before(root.path(), UNIX_EPOCH), 0);
        assert_eq!(
            cleanup_stale_temporaries_before(
                root.path(),
                SystemTime::now() + Duration::from_secs(1),
            ),
            owned.len()
        );
        assert!(owned.iter().all(|path| !path.exists()));
        assert!(unrelated.exists());
        assert!(malformed.exists());
        assert!(outside.exists());
    }

    #[test]
    fn plugin_cleanup_accepts_only_exact_journal_and_integrity_names() {
        let root = tempdir().unwrap();
        let token = "00000000-0000-0000-0000-000000000000";
        let digest = "a".repeat(64);
        let journal = root
            .path()
            .join("NativeConnector/PluginRuntime/InvocationJournal");
        let integrity = root.path().join("NativeConnector/Plugins/.integrity");
        fs::create_dir_all(&journal).unwrap();
        fs::create_dir_all(&integrity).unwrap();
        let journal_temporary = journal.join(format!(".{token}.tmp"));
        let integrity_temporary = integrity.join(format!(".{digest}-{token}.tmp"));
        let committed_journal = journal.join(format!("{digest}.intent.json"));
        let committed_integrity = integrity.join(format!("{digest}.json"));
        let malformed_integrity = integrity.join(format!(".not-a-digest-{token}.tmp"));
        for path in [
            &journal_temporary,
            &integrity_temporary,
            &committed_journal,
            &committed_integrity,
            &malformed_integrity,
        ] {
            fs::write(path, b"value").unwrap();
        }

        assert_eq!(
            cleanup_stale_temporaries_before(
                root.path(),
                SystemTime::now() + Duration::from_secs(1),
            ),
            2
        );
        assert!(!journal_temporary.exists());
        assert!(!integrity_temporary.exists());
        assert!(committed_journal.exists());
        assert!(committed_integrity.exists());
        assert!(malformed_integrity.exists());
    }

    #[test]
    fn plugin_cleanup_removes_only_stale_canonical_staging_trees() {
        let root = tempdir().unwrap();
        let token = "00000000-0000-0000-0000-000000000000";
        let source_staging = root.path().join("NativeConnector/Plugins/.source-staging");
        let install_staging = root.path().join("NativeConnector/Plugins/.staging");
        let source_tree = source_staging.join(token).join("package/nested");
        let install_tree = install_staging.join(token).join("package");
        let malformed = install_staging.join("NOT-A-UUID");
        let linked_tree = source_staging
            .join("11111111-1111-1111-1111-111111111111")
            .join("package");
        let committed = root
            .path()
            .join("NativeConnector/Plugins")
            .join("a".repeat(64));
        for directory in [
            &source_tree,
            &install_tree,
            &malformed,
            &linked_tree,
            &committed,
        ] {
            fs::create_dir_all(directory).unwrap();
            fs::write(directory.join("value.json"), b"value").unwrap();
        }
        let outside = root.path().join("outside.json");
        fs::write(&outside, b"outside").unwrap();
        fs::hard_link(&outside, linked_tree.join("linked.json")).unwrap();

        assert_eq!(cleanup_stale_temporaries_before(root.path(), UNIX_EPOCH), 0);
        assert_eq!(
            cleanup_stale_temporaries_before(
                root.path(),
                SystemTime::now() + Duration::from_secs(1),
            ),
            2
        );
        assert!(!source_staging.join(token).exists());
        assert!(!install_staging.join(token).exists());
        assert!(malformed.exists());
        assert!(linked_tree.exists());
        assert!(outside.exists());
        assert!(committed.exists());
    }
}

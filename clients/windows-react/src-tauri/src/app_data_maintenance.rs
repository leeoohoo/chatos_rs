use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

use crate::app_owned_export::try_remove_stale_owned_file;

const STALE_APP_DATA_TEMPORARY_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAXIMUM_ENTRIES_PER_SCOPE: usize = 2_000;

const PREFERENCE_PREFIXES: &[&str] =
    &[".general-settings-", ".global-utilities-", ".quick-search-"];
const PET_PREFIXES: &[&str] = &[".preferences-", ".project-favorites-"];
const PROJECT_RUN_PREFIXES: &[&str] = &[".preferences-"];
const REMOTE_CONNECTION_PREFIXES: &[&str] = &[".credential-index-"];
const CLIPBOARD_PREFIXES: &[&str] = &[".clipboard-"];
const APPROVAL_PREFIXES: &[&str] = &[".approval-"];

const CLEANUP_SCOPES: &[CleanupScope] = &[
    CleanupScope {
        components: &["preferences"],
        prefixes: PREFERENCE_PREFIXES,
    },
    CleanupScope {
        components: &["pet"],
        prefixes: PET_PREFIXES,
    },
    CleanupScope {
        components: &["project-run"],
        prefixes: PROJECT_RUN_PREFIXES,
    },
    CleanupScope {
        components: &["remote-connections"],
        prefixes: REMOTE_CONNECTION_PREFIXES,
    },
    CleanupScope {
        components: &["clipboard-history", "Payloads"],
        prefixes: CLIPBOARD_PREFIXES,
    },
    CleanupScope {
        components: &["local-agent", "approval-decisions"],
        prefixes: APPROVAL_PREFIXES,
    },
];

struct CleanupScope {
    components: &'static [&'static str],
    prefixes: &'static [&'static str],
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
    CLEANUP_SCOPES
        .iter()
        .map(|scope| cleanup_scope(root, scope, cutoff))
        .fold(0usize, usize::saturating_add)
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
        if canonical_temporary_name(&name, scope.prefixes)
            && try_remove_stale_owned_file(&entry.path(), cutoff)
        {
            removed = removed.saturating_add(1);
        }
    }
    removed
}

fn canonical_temporary_name(name: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        name.strip_prefix(prefix)
            .and_then(|value| value.strip_suffix(".tmp"))
            .is_some_and(|value| Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value))
    })
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
}

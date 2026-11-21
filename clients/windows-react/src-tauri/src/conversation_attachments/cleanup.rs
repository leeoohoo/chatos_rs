use std::{fs, path::Path, time::SystemTime};

use uuid::Uuid;

use super::{platform::is_windows_reparse_point, MAXIMUM_CLEANUP_DEPTH, MAXIMUM_CLEANUP_ENTRIES};
use crate::app_owned_export::try_remove_stale_owned_file;

pub(super) fn cleanup_attachment_temporaries(root: &Path, cutoff: SystemTime) -> usize {
    let mut visited = 0;
    let mut removed = 0;
    cleanup_attachment_directory(root, 0, cutoff, &mut visited, &mut removed);
    removed
}

fn cleanup_attachment_directory(
    directory: &Path,
    depth: usize,
    cutoff: SystemTime,
    visited: &mut usize,
    removed: &mut usize,
) {
    if *visited >= MAXIMUM_CLEANUP_ENTRIES || validate_cleanup_directory(directory).is_err() {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if *visited >= MAXIMUM_CLEANUP_ENTRIES {
            return;
        }
        *visited += 1;
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_dir()
            && !metadata.file_type().is_symlink()
            && !is_windows_reparse_point(&metadata)
            && depth < MAXIMUM_CLEANUP_DEPTH
        {
            cleanup_attachment_directory(&path, depth + 1, cutoff, visited, removed);
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !canonical_attachment_temporary_name(&name)
            || !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || is_windows_reparse_point(&metadata)
            || !metadata.modified().is_ok_and(|modified| modified <= cutoff)
        {
            continue;
        }
        if try_remove_stale_owned_file(&path, cutoff) {
            *removed += 1;
        }
    }
}

fn validate_cleanup_directory(path: &Path) -> Result<(), ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(&metadata)
    {
        Ok(())
    } else {
        Err(())
    }
}

fn canonical_attachment_temporary_name(name: &str) -> bool {
    let legacy_token = name
        .strip_suffix(".json.tmp")
        .or_else(|| name.strip_suffix(".tmp"));
    if legacy_token.is_some_and(valid_attachment_temporary_token) {
        return true;
    }
    [".attachment-payload-", ".attachment-metadata-"]
        .iter()
        .any(|prefix| {
            name.strip_prefix(prefix)
                .and_then(|value| value.strip_suffix(".tmp"))
                .is_some_and(valid_attachment_temporary_token)
        })
}

fn valid_attachment_temporary_token(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value)
}

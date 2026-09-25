// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use chatos_local_workspace::LOCAL_CONNECTOR_ROOT_PREFIX;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

const CHATOS_DIR_NAME: &str = ".chatos";
const CACHE_DIR_NAME: &str = "cache";
pub fn is_local_connector_project_root(project_root: &str) -> bool {
    let trimmed = project_root.trim();
    trimmed == "local://connector" || trimmed.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX)
}

fn normalize_cache_relative_path(relative_path: &str) -> Result<PathBuf, String> {
    let trimmed = relative_path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("cache relative path cannot be empty".to_string());
    }

    let candidate = Path::new(trimmed.as_str());
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("cache relative path is invalid".to_string());
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err("cache relative path cannot be empty".to_string());
    }
    Ok(normalized)
}

pub fn project_cache_root(project_root: &str) -> PathBuf {
    Path::new(project_root)
        .join(CHATOS_DIR_NAME)
        .join(CACHE_DIR_NAME)
}

pub fn project_cache_file_path(project_root: &str, relative_path: &str) -> Result<PathBuf, String> {
    if is_local_connector_project_root(project_root) {
        return Err("local connector project cache is not stored on the server".to_string());
    }
    let normalized_relative = normalize_cache_relative_path(relative_path)?;
    Ok(project_cache_root(project_root).join(normalized_relative))
}

pub fn read_cache_json<T>(project_root: &str, relative_path: &str) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    if is_local_connector_project_root(project_root) {
        return Ok(None);
    }
    let path = project_cache_file_path(project_root, relative_path)?;
    if !path.is_file() {
        return Ok(None);
    }
    #[cfg(unix)]
    let mut file = open_cache_file_without_symlinks(&path).map_err(|err| err.to_string())?;
    #[cfg(not(unix))]
    let mut file = fs::File::open(&path).map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    serde_json::from_slice::<T>(&bytes)
        .map(Some)
        .map_err(|err| err.to_string())
}

#[cfg(unix)]
fn open_cache_file_without_symlinks(path: &Path) -> std::io::Result<fs::File> {
    use crate::core::fs_open::open_directory_without_symlinks;
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;

    // The project context is already canonical. Resolving it again here would
    // follow replaced ancestors; instead hold each directory while opening the next.
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "expected a cache parent")
    })?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected a cache file name",
        )
    })?;
    let name = CString::new(name.as_bytes())?;
    let directory = open_directory_without_symlinks(parent)?;
    // SAFETY: directory owns a live descriptor and name is one NUL-terminated
    // component. O_CREAT is absent, so no mode argument is required.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            // A replaced FIFO must not wait for a writer before type validation.
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful openat returns a fresh descriptor owned only here.
    let file = unsafe { fs::File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    // The earlier is_file precheck may race. Validate the actual object before
    // returning its handle for JSON reads; RAII closes rejected descriptors.
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "cache target is not a regular file",
        ));
    }
    // O_NOFOLLOW does not reject hard links. Inspect the opened inode before
    // reading JSON shared with another path. This does not prevent concurrent
    // link creation after the check.
    if metadata.nlink() > 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "cache target has multiple hard links",
        ));
    }
    Ok(file)
}

pub fn write_cache_json<T>(project_root: &str, relative_path: &str, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if is_local_connector_project_root(project_root) {
        return Ok(());
    }
    let path = project_cache_file_path(project_root, relative_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Reject leaf links at open, and never wait for a FIFO reader before
        // we can check the opened object's type. A path precheck would race.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options
        .open(path)
        .and_then(|mut file| {
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "cache target is not a regular file",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                // O_NOFOLLOW does not reject hard links. Check the opened
                // inode before truncating any file with another name. This
                // does not prevent concurrent link creation after the check.
                if metadata.nlink() > 1 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "cache target has multiple hard links",
                    ));
                }
            }
            // Validate before truncating, then write through the same handle.
            file.set_len(0)?;
            file.write_all(&bytes)
        })
        .map_err(|err| err.to_string())
}

pub fn cache_key(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.trim().as_bytes());
    let hex = hex::encode(hasher.finalize());
    hex.chars().take(24).collect()
}

#[cfg(test)]
#[path = "project_local_cache_read_tests.rs"]
mod read_tests;

#[cfg(test)]
#[path = "project_local_cache_write_tests.rs"]
mod write_tests;

#[cfg(all(test, unix))]
#[path = "project_local_cache_special_tests.rs"]
mod special_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_connector_roots_do_not_resolve_to_server_cache_paths() {
        let root = "local://connector/device-1/workspace-1/apps/web";

        assert!(is_local_connector_project_root(root));
        assert!(project_cache_file_path(root, "project_run/catalog.json").is_err());
        assert!(
            read_cache_json::<serde_json::Value>(root, "project_run/catalog.json")
                .unwrap()
                .is_none()
        );
        write_cache_json(
            root,
            "project_run/catalog.json",
            &serde_json::json!({"ok": true}),
        )
        .unwrap();
    }

    #[test]
    fn normal_project_roots_still_resolve_project_cache_paths() {
        let path =
            project_cache_file_path("/tmp/example-project", "project_run/catalog.json").unwrap();
        assert_eq!(
            path,
            Path::new("/tmp/example-project")
                .join(".chatos")
                .join("cache")
                .join("project_run")
                .join("catalog.json")
        );
    }
}

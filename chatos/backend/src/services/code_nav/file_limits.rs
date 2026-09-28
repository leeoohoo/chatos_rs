// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

pub(crate) const CODE_NAV_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[cfg(all(test, unix))]
#[path = "file_limits_symlink_tests.rs"]
mod symlink_tests;

#[cfg(all(test, unix))]
#[path = "file_limits_special_tests.rs"]
mod special_tests;

#[cfg(all(test, unix))]
#[path = "file_limits_hardlink_tests.rs"]
mod hardlink_tests;

pub(crate) fn read_code_nav_file_to_string(path: &Path) -> Result<String, String> {
    let file = open_code_nav_file(path)?;
    let mut bytes = Vec::new();
    let mut reader = BufReader::new(file).take(CODE_NAV_MAX_FILE_BYTES.saturating_add(1));
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| format!("read code-nav file failed: {err}"))?;
    ensure_code_nav_file_within_limit(path, bytes.len() as u64)?;
    String::from_utf8(bytes).map_err(|err| format!("read code-nav file as utf-8 failed: {err}"))
}

pub(crate) fn read_code_nav_line_preview(
    path: &Path,
    line: usize,
    max_chars: usize,
) -> Result<String, String> {
    if line == 0 {
        return Ok(String::new());
    }

    let file = open_code_nav_file(path)?;
    read_code_nav_line_preview_from_reader(path, file, line, max_chars)
}

fn read_code_nav_line_preview_from_reader(
    path: &Path,
    source: impl Read,
    line: usize,
    max_chars: usize,
) -> Result<String, String> {
    // The file can grow after the opener's metadata check. Bound the source
    // before buffering and share one budget across all scanned lines.
    let mut reader = BufReader::new(source.take(CODE_NAV_MAX_FILE_BYTES + 1));
    let mut total_bytes = 0;
    for current_line in 1..=line {
        let mut bytes = Vec::new();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|err| format!("read code-nav line failed: {err}"))?;
        total_bytes += read as u64;
        ensure_code_nav_file_within_limit(path, total_bytes)?;
        if read == 0 {
            return Ok(String::new());
        }
        if current_line == line {
            while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                bytes.pop();
            }
            let line = String::from_utf8_lossy(bytes.as_slice());
            return Ok(truncate_preview(line.as_ref(), max_chars));
        }
    }

    Ok(String::new())
}

pub(crate) fn truncate_preview(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn open_code_nav_file(path: &Path) -> Result<File, String> {
    #[cfg(unix)]
    let file = open_canonical_code_nav_file(path).map_err(|err| err.to_string())?;
    #[cfg(not(unix))]
    let file = File::open(path).map_err(|err| err.to_string())?;
    let metadata = file.metadata().map_err(|err| err.to_string())?;
    if !metadata.is_file() {
        return Err("code-nav path is not a regular file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Canonical paths and O_NOFOLLOW cannot distinguish hard links to
        // files outside the project. Check the opened inode before reading;
        // this does not prevent links created after the metadata check.
        if metadata.nlink() > 1 {
            return Err("code-nav file has multiple hard links".to_string());
        }
    }
    ensure_code_nav_file_within_limit(path, metadata.len())?;
    Ok(file)
}

#[cfg(unix)]
fn open_canonical_code_nav_file(path: &Path) -> std::io::Result<File> {
    use crate::core::fs_open::open_directory_without_symlinks;
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    // Callers supply paths from the validated canonical project context. Do
    // not canonicalize again: that could follow a replaced ancestor outside it.
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "expected a file parent")
    })?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "expected a file name")
    })?;
    let name = CString::new(name.as_bytes())?;
    let directory = open_directory_without_symlinks(parent)?;
    // SAFETY: directory owns a live descriptor and name is one NUL-terminated
    // component. O_CREAT is absent, so no mode argument is required.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            // Do not wait for a FIFO peer before the handle's type can be
            // checked. O_NONBLOCK does not change regular-file reads.
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful openat returns a fresh descriptor owned only here.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn ensure_code_nav_file_within_limit(path: &Path, actual_bytes: u64) -> Result<(), String> {
    if actual_bytes > CODE_NAV_MAX_FILE_BYTES {
        return Err(format!(
            "code-nav file exceeds limit: {} bytes > {} bytes ({})",
            actual_bytes,
            CODE_NAV_MAX_FILE_BYTES,
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "file_limits_growth_tests.rs"]
mod growth_tests;

#[cfg(test)]
mod tests {
    use super::{
        ensure_code_nav_file_within_limit, read_code_nav_line_preview, truncate_preview,
        CODE_NAV_MAX_FILE_BYTES,
    };
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_file(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "code_nav_file_limits_test_{}.txt",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, content).expect("write temp file");
        fs::canonicalize(path).expect("canonical temp file")
    }

    #[test]
    fn code_nav_file_limit_accepts_boundary_size() {
        assert!(ensure_code_nav_file_within_limit(
            PathBuf::from("source.rs").as_path(),
            CODE_NAV_MAX_FILE_BYTES
        )
        .is_ok());
    }

    #[test]
    fn code_nav_file_limit_rejects_oversized_file() {
        let err = ensure_code_nav_file_within_limit(
            PathBuf::from("source.rs").as_path(),
            CODE_NAV_MAX_FILE_BYTES + 1,
        )
        .expect_err("oversized file should fail");

        assert!(err.contains("code-nav file exceeds limit"));
    }

    #[test]
    fn line_preview_reads_requested_line_only() {
        let path = make_temp_file("first\nsecond line\nthird\n");
        let preview = read_code_nav_line_preview(&path, 2, 6).expect("read line preview");

        assert_eq!(preview, "second");

        fs::remove_file(path).ok();
    }

    #[test]
    fn preview_truncates_on_char_boundary() {
        assert_eq!(truncate_preview("你好世界", 2), "你好");
    }
}

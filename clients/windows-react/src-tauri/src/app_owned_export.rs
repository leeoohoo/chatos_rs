use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const STALE_EXPORT_AGE: Duration = Duration::from_secs(24 * 60 * 60);

pub(crate) fn export_bytes(
    destination: &Path,
    temporary_prefix: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let leaf = destination
        .file_name()
        .filter(|value| valid_leaf_name(value.to_string_lossy().as_ref()))
        .ok_or_else(export_error)?;
    if !valid_temporary_prefix(temporary_prefix) {
        return Err(export_error());
    }
    let requested_parent = destination.parent().ok_or_else(export_error)?;
    let parent = fs::canonicalize(requested_parent).map_err(|_| export_error())?;
    validate_directory(&parent)?;
    cleanup_stale_exports(&parent, temporary_prefix);

    let destination = parent.join(leaf);
    validate_existing_destination(&destination)?;
    let temporary = parent.join(format!(
        "{temporary_prefix}{}.tmp",
        Uuid::new_v4().to_string().to_lowercase()
    ));
    let mut cleanup = TemporaryExport::new(temporary.clone());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(&temporary).map_err(|_| export_error())?;
    for chunk in bytes.chunks(256 * 1_024) {
        file.write_all(chunk).map_err(|_| export_error())?;
    }
    file.flush().map_err(|_| export_error())?;
    file.sync_all().map_err(|_| export_error())?;
    let metadata = file.metadata().map_err(|_| export_error())?;
    if !is_regular_non_reparse(&metadata) || metadata.len() != bytes.len() as u64 {
        return Err(export_error());
    }
    drop(file);

    let current_parent = fs::canonicalize(requested_parent).map_err(|_| export_error())?;
    if current_parent != parent {
        return Err(export_error());
    }
    validate_existing_destination(&destination)?;
    replace_file(&temporary, &destination).map_err(|_| export_error())?;
    cleanup.installed = true;
    Ok(())
}

struct TemporaryExport {
    path: PathBuf,
    installed: bool,
}

impl TemporaryExport {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            installed: false,
        }
    }
}

impl Drop for TemporaryExport {
    fn drop(&mut self) {
        if !self.installed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn cleanup_stale_exports(parent: &Path, prefix: &str) {
    let cutoff = SystemTime::now()
        .checked_sub(STALE_EXPORT_AGE)
        .unwrap_or(UNIX_EPOCH);
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if canonical_temporary_name(&name, prefix) {
            let _ = try_remove_stale_owned_file(&entry.path(), cutoff);
        }
    }
}

#[cfg(windows)]
pub(crate) fn try_remove_stale_owned_file(path: &Path, cutoff: SystemTime) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FileDispositionInfo, GetFileInformationByHandle,
            SetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_DISPOSITION_INFO,
        },
    };

    const DELETE_ACCESS: u32 = 0x0001_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const WINDOWS_TO_UNIX_SECONDS: u64 = 11_644_473_600;

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            DELETE_ACCESS | FILE_READ_ATTRIBUTES | SYNCHRONIZE_ACCESS,
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
    let ticks = ((information.ftLastWriteTime.dwHighDateTime as u64) << 32)
        | information.ftLastWriteTime.dwLowDateTime as u64;
    let modified = UNIX_EPOCH
        .checked_add(Duration::from_secs(
            ticks
                .checked_div(10_000_000)
                .unwrap_or_default()
                .saturating_sub(WINDOWS_TO_UNIX_SECONDS),
        ))
        .unwrap_or(UNIX_EPOCH);
    let safe = valid
        && information.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
            == 0
        && information.nNumberOfLinks == 1
        && modified <= cutoff;
    let removed = if safe {
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        (unsafe {
            SetFileInformationByHandle(
                handle,
                FileDispositionInfo,
                (&raw const disposition).cast(),
                u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO>()).unwrap_or_default(),
            )
        }) != 0
    } else {
        false
    };
    let _ = unsafe { CloseHandle(handle) };
    removed
}

#[cfg(not(windows))]
pub(crate) fn try_remove_stale_owned_file(path: &Path, cutoff: SystemTime) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if is_regular_non_reparse(&metadata)
        && metadata.modified().is_ok_and(|modified| modified <= cutoff)
    {
        return fs::remove_file(path).is_ok();
    }
    false
}

fn canonical_temporary_name(name: &str, prefix: &str) -> bool {
    let Some(value) = name
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(".tmp"))
    else {
        return false;
    };
    Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value)
}

fn valid_temporary_prefix(value: &str) -> bool {
    value.starts_with(".chatos-")
        && value.ends_with('-')
        && value.len() <= 80
        && !value.contains('/')
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
}

fn valid_leaf_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
}

fn validate_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| export_error())?;
    if metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(&metadata)
    {
        Ok(())
    } else {
        Err(export_error())
    }
}

fn validate_existing_destination(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_regular_non_reparse(&metadata) => Ok(()),
        Ok(_) => Err(export_error()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(export_error()),
    }
}

fn is_regular_non_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
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

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn export_error() -> String {
    "无法安全导出文件。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_export_replaces_the_destination_without_leaving_a_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("artifact.md");
        fs::write(&destination, b"old").unwrap();
        export_bytes(&destination, ".chatos-artifact-export-", b"complete export").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"complete export");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn cleanup_only_removes_canonical_owned_temporary_names() {
        let directory = tempfile::tempdir().unwrap();
        let owned = directory
            .path()
            .join(".chatos-artifact-export-00000000-0000-0000-0000-000000000000.tmp");
        let unrelated = directory
            .path()
            .join(".chatos-artifact-export-not-owned.tmp");
        fs::write(&owned, b"owned").unwrap();
        fs::write(&unrelated, b"unrelated").unwrap();
        assert!(try_remove_stale_owned_file(
            &owned,
            SystemTime::now() + Duration::from_secs(1)
        ));
        assert!(!owned.exists());
        assert!(unrelated.exists());
        assert!(canonical_temporary_name(
            ".chatos-artifact-export-00000000-0000-0000-0000-000000000000.tmp",
            ".chatos-artifact-export-"
        ));
        assert!(!canonical_temporary_name(
            ".chatos-artifact-export-not-owned.tmp",
            ".chatos-artifact-export-"
        ));
    }
}

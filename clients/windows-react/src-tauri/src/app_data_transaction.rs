use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) fn replace_owned_file(
    directory: &Path,
    destination: &Path,
    temporary_prefix: &str,
    bytes: &[u8],
) -> io::Result<()> {
    validate_temporary_prefix(temporary_prefix)?;
    validate_directory(directory)?;
    if destination.parent() != Some(directory) {
        return Err(invalid_input());
    }
    validate_destination(destination)?;

    let temporary = directory.join(format!("{temporary_prefix}{}.tmp", uuid::Uuid::new_v4()));
    let mut cleanup = TemporaryTransaction::new(temporary.clone());
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    let metadata = fs::symlink_metadata(&temporary)?;
    if !regular_non_reparse_file(&metadata) || metadata.len() != bytes.len() as u64 {
        return Err(io::Error::other("invalid transaction file"));
    }
    drop(file);
    replace_file(&temporary, destination)?;
    cleanup.committed = true;
    Ok(())
}

struct TemporaryTransaction {
    path: PathBuf,
    committed: bool,
}

impl TemporaryTransaction {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }
}

impl Drop for TemporaryTransaction {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn validate_temporary_prefix(value: &str) -> io::Result<()> {
    if value.starts_with('.')
        && value.ends_with('-')
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'.' | b'-'))
    {
        Ok(())
    } else {
        Err(invalid_input())
    }
}

fn validate_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(&metadata)
    {
        Ok(())
    } else {
        Err(invalid_input())
    }
}

fn validate_destination(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if regular_non_reparse_file(&metadata) => Ok(()),
        Ok(_) => Err(invalid_input()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn regular_non_reparse_file(metadata: &fs::Metadata) -> bool {
    metadata.is_file() && !metadata.file_type().is_symlink() && !is_windows_reparse_point(metadata)
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

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
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
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

fn invalid_input() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid app data transaction")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_is_atomic_and_leaves_no_owned_temporary() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("state.json");
        fs::write(&destination, b"old").unwrap();

        replace_owned_file(directory.path(), &destination, ".connector-state-", b"new").unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(replace_owned_file(
            directory.path(),
            &directory.path().join("nested/state.json"),
            ".connector-state-",
            b"invalid",
        )
        .is_err());
    }
}

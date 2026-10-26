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
    write_owned_file(
        directory,
        destination,
        temporary_prefix,
        bytes,
        CommitMode::Replace,
    )
}

pub(crate) fn create_owned_file(
    directory: &Path,
    destination: &Path,
    temporary_prefix: &str,
    bytes: &[u8],
) -> io::Result<()> {
    write_owned_file(
        directory,
        destination,
        temporary_prefix,
        bytes,
        CommitMode::CreateNew,
    )
}

enum CommitMode {
    Replace,
    CreateNew,
}

fn write_owned_file(
    directory: &Path,
    destination: &Path,
    temporary_prefix: &str,
    bytes: &[u8],
    mode: CommitMode,
) -> io::Result<()> {
    validate_temporary_prefix(temporary_prefix)?;
    validate_directory(directory)?;
    if destination.parent() != Some(directory) {
        return Err(invalid_input());
    }
    validate_destination(destination, &mode)?;

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
    commit_file(&temporary, destination, mode)?;
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

fn validate_destination(path: &Path, mode: &CommitMode) -> io::Result<()> {
    match (fs::symlink_metadata(path), mode) {
        (Ok(metadata), CommitMode::Replace) if regular_non_reparse_file(&metadata) => Ok(()),
        (Ok(_), CommitMode::Replace) => Err(invalid_input()),
        (Ok(_), CommitMode::CreateNew) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "app data destination already exists",
        )),
        (Err(error), _) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        (Err(error), _) => Err(error),
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
fn commit_file(source: &Path, destination: &Path, mode: CommitMode) -> io::Result<()> {
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
    let flags = match mode {
        CommitMode::Replace => MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        CommitMode::CreateNew => MOVEFILE_WRITE_THROUGH,
    };
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn commit_file(source: &Path, destination: &Path, mode: CommitMode) -> io::Result<()> {
    match mode {
        CommitMode::Replace => fs::rename(source, destination),
        CommitMode::CreateNew => {
            fs::hard_link(source, destination)?;
            let _ = fs::remove_file(source);
            Ok(())
        }
    }
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

    #[test]
    fn create_new_is_atomic_and_never_replaces_an_existing_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("journal.json");

        create_owned_file(
            directory.path(),
            &destination,
            ".project-creation-",
            b"first",
        )
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"first");
        assert!(create_owned_file(
            directory.path(),
            &destination,
            ".project-creation-",
            b"second",
        )
        .is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"first");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

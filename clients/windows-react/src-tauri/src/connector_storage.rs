use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::app_data_transaction::ensure_owned_directory;

pub(crate) fn confined_connector_directory(app_data_directory: &Path) -> io::Result<PathBuf> {
    ensure_owned_directory(app_data_directory)?;
    reject_link_or_non_directory(app_data_directory)?;
    let app_data = fs::canonicalize(app_data_directory)?;
    let requested = app_data.join("NativeConnector");
    ensure_owned_directory(&requested)?;
    reject_link_or_non_directory(&requested)?;
    let connector = fs::canonicalize(requested)?;
    if connector.starts_with(&app_data) {
        Ok(connector)
    } else {
        Err(io::Error::other("connector directory escaped app data"))
    }
}

pub(crate) fn reject_link_or_non_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                || is_reparse_point(&metadata)
                || !metadata.is_file() =>
        {
            Err(io::Error::other("connector state is not a regular file"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn reject_link_or_non_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() && !is_reparse_point(&metadata) {
        Ok(())
    } else {
        Err(io::Error::other(
            "connector directory is not a regular directory",
        ))
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

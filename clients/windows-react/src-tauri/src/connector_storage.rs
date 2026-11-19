use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) fn confined_connector_directory(app_data_directory: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(app_data_directory)?;
    let app_data = fs::canonicalize(app_data_directory)?;
    let requested = app_data.join("NativeConnector");
    fs::create_dir_all(&requested)?;
    let connector = fs::canonicalize(requested)?;
    if connector.starts_with(&app_data) {
        Ok(connector)
    } else {
        Err(io::Error::other("connector directory escaped app data"))
    }
}

pub(crate) fn reject_link_or_non_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(io::Error::other("connector state is not a regular file"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

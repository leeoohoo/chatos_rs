use super::{plugin_catalog_error, plugin_runtime_error};
use crate::app_data_transaction::ensure_owned_directory;
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

pub(super) fn resolve_non_linked_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.len() > 4_096 || relative.chars().any(char::is_control) {
        return Err(plugin_catalog_error());
    }
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(plugin_catalog_error());
    }
    let canonical_root = root.canonicalize().map_err(|_| plugin_catalog_error())?;
    let mut candidate = canonical_root.clone();
    for component in relative.components() {
        if let Component::Normal(component) = component {
            candidate.push(component);
            let metadata = fs::symlink_metadata(&candidate).map_err(|_| plugin_catalog_error())?;
            if is_link_or_reparse_point(&metadata) {
                return Err(plugin_catalog_error());
            }
        }
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
        return Err(plugin_catalog_error());
    }
    Ok(canonical)
}

pub(super) fn create_confined_directory(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(plugin_runtime_error());
    }
    ensure_owned_directory(root).map_err(|_| plugin_runtime_error())?;
    let requested_root_metadata = fs::symlink_metadata(root).map_err(|_| plugin_runtime_error())?;
    if !requested_root_metadata.is_dir() || is_link_or_reparse_point(&requested_root_metadata) {
        return Err(plugin_runtime_error());
    }
    let root = root.canonicalize().map_err(|_| plugin_runtime_error())?;
    if is_link_or_reparse_point(&fs::symlink_metadata(&root).map_err(|_| plugin_runtime_error())?) {
        return Err(plugin_runtime_error());
    }
    let mut current = root.clone();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(plugin_runtime_error());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if !metadata.is_dir() || is_link_or_reparse_point(&metadata) {
                    return Err(plugin_runtime_error());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|_| plugin_runtime_error())?;
            }
            Err(_) => return Err(plugin_runtime_error()),
        }
    }
    let canonical = current.canonicalize().map_err(|_| plugin_runtime_error())?;
    if !canonical.starts_with(&root) {
        return Err(plugin_runtime_error());
    }
    Ok(canonical)
}

pub(super) fn bounded_file(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| plugin_runtime_error())?;
    if !metadata.is_file() || is_link_or_reparse_point(&metadata) || metadata.len() > maximum {
        return Err(plugin_runtime_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|file| file.take(maximum + 1).read_to_end(&mut bytes))
        .map_err(|_| plugin_runtime_error())?;
    if bytes.len() as u64 > maximum {
        return Err(plugin_runtime_error());
    }
    Ok(bytes)
}

#[cfg(windows)]
pub(super) fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(super) fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

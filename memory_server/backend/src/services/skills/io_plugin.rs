use std::fs;
use std::path::{Path as FsPath, PathBuf};

use super::io_common::{ensure_dir, normalize_plugin_source, run_blocking_result};
use super::io_helpers::has_parent_path_component;

pub async fn copy_plugin_source_from_repo_async(
    repo_root: PathBuf,
    plugins_root: PathBuf,
    source: String,
) -> Result<String, String> {
    run_blocking_result(move || {
        copy_plugin_source_from_repo(repo_root.as_path(), plugins_root.as_path(), source.as_str())
    })
    .await
}

fn copy_plugin_source_from_repo(
    repo_root: &FsPath,
    plugins_root: &FsPath,
    source: &str,
) -> Result<String, String> {
    let normalized = normalize_plugin_source(source);
    if normalized.is_empty() {
        return Err("plugin source is empty".to_string());
    }
    if has_parent_path_component(normalized.as_str()) {
        return Err("plugin source cannot contain ..".to_string());
    }

    let src = repo_root.join(normalized.as_str());
    if !src.exists() {
        return Err(format!(
            "plugin source not found in repository: {}",
            normalized
        ));
    }

    let dest_rel = plugin_install_destination(normalized.as_str());
    if dest_rel.is_empty() {
        return Err("plugin source normalization failed".to_string());
    }

    let dest = plugins_root.join(dest_rel.as_str());
    copy_path(src.as_path(), dest.as_path())?;
    Ok(dest_rel)
}

fn copy_path(src: &FsPath, dest: &FsPath) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("source not found: {}", src.to_string_lossy()));
    }

    if dest.exists() {
        if dest.is_dir() {
            fs::remove_dir_all(dest).map_err(|err| err.to_string())?;
        } else {
            fs::remove_file(dest).map_err(|err| err.to_string())?;
        }
    }

    if src.is_file() {
        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        fs::copy(src, dest).map_err(|err| err.to_string())?;
        return Ok(());
    }

    ensure_dir(dest)?;
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let next = dest.join(entry.file_name());
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        if file_type.is_dir() {
            copy_path(path.as_path(), next.as_path())?;
        } else if file_type.is_file() {
            if let Some(parent) = next.parent() {
                ensure_dir(parent)?;
            }
            fs::copy(path.as_path(), next.as_path()).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

fn plugin_install_destination(source: &str) -> String {
    let normalized = normalize_plugin_source(source);
    if let Some(stripped) = normalized.strip_prefix("plugins/") {
        stripped.trim_matches('/').to_string()
    } else {
        normalized
    }
}

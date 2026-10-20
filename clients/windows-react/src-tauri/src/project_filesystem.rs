use serde::Serialize;
use std::{fs, path::Path, time::UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use crate::{
    host_lifecycle::validate_owner,
    platform_execution_context::{resolve_execution_directory, resolve_project_root},
};

const MAXIMUM_DIRECTORY_ENTRIES: usize = 5_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileEntry {
    name: String,
    path: String,
    is_directory: bool,
    is_writable: bool,
    size: Option<u64>,
    modified_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryListing {
    project_id: String,
    path: String,
    parent_path: Option<String>,
    is_writable: bool,
    entries: Vec<ProjectFileEntry>,
    is_truncated: bool,
}

#[tauri::command]
pub async fn load_project_directory(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    path: String,
    _force_refresh: bool,
) -> Result<ProjectDirectoryListing, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let relative_path = normalized_relative_path(&path)?;
    let app_data_directory = app.path().app_data_dir().map_err(|_| directory_error())?;
    tauri::async_runtime::spawn_blocking(move || {
        let connector_directory = app_data_directory.join("NativeConnector");
        let resolved = resolve_project_root(&connector_directory, &owner_user_id, &project_id)?;
        list_directory(&resolved.root, &project_id, &relative_path)
    })
    .await
    .map_err(|_| "项目目录读取任务异常退出。".to_owned())?
}

fn list_directory(
    project_root: &Path,
    project_id: &str,
    relative_path: &str,
) -> Result<ProjectDirectoryListing, String> {
    let directory = resolve_execution_directory(
        project_root,
        if relative_path.is_empty() {
            "."
        } else {
            relative_path
        },
    )?;
    let mut entries = Vec::new();
    let mut is_truncated = false;
    let children = fs::read_dir(&directory).map_err(|_| directory_error())?;
    for child in children {
        if entries.len() >= MAXIMUM_DIRECTORY_ENTRIES {
            is_truncated = true;
            break;
        }
        let child = child.map_err(|_| directory_error())?;
        let metadata = fs::symlink_metadata(child.path()).map_err(|_| directory_error())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let Some(name) = child.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_component(&name) {
            continue;
        }
        let is_directory = metadata.is_dir();
        if !is_directory && !metadata.is_file() {
            continue;
        }
        let path = if relative_path.is_empty() {
            name.clone()
        } else {
            format!("{relative_path}/{name}")
        };
        entries.push(ProjectFileEntry {
            name,
            path,
            is_directory,
            is_writable: !metadata.permissions().readonly(),
            size: (!is_directory).then_some(metadata.len()),
            modified_at_unix_ms: metadata.modified().ok().and_then(unix_milliseconds),
        });
    }
    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    let root_metadata = fs::metadata(project_root).map_err(|_| directory_error())?;
    Ok(ProjectDirectoryListing {
        project_id: project_id.to_owned(),
        path: relative_path.to_owned(),
        parent_path: parent_path(relative_path),
        is_writable: !root_metadata.permissions().readonly(),
        entries,
        is_truncated,
    })
}

fn normalized_relative_path(input: &str) -> Result<String, String> {
    if input.len() > 4_096
        || input.trim() != input
        || input.contains('\\')
        || input.contains(':')
        || input.chars().any(char::is_control)
    {
        return Err(invalid_path());
    }
    if input.is_empty() {
        return Ok(String::new());
    }
    if input
        .split('/')
        .any(|component| !valid_component(component))
    {
        return Err(invalid_path());
    }
    Ok(input.to_owned())
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 1_024
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value.chars().any(char::is_control)
}

fn parent_path(path: &str) -> Option<String> {
    if path.is_empty() {
        None
    } else {
        Some(
            path.rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or_default()
                .to_owned(),
        )
    }
}

fn unix_milliseconds(time: std::time::SystemTime) -> Option<i64> {
    let value = time.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(value).ok()
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        Err(format!("{field} is invalid"))
    } else {
        Ok(value.to_owned())
    }
}

fn invalid_path() -> String {
    "项目目录路径无效。".into()
}

fn directory_error() -> String {
    "无法读取项目目录。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_direct_children_in_macos_directory_order_without_absolute_paths() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Sources")).unwrap();
        fs::write(root.path().join("README.md"), "hello").unwrap();
        fs::write(root.path().join("alpha.txt"), "a").unwrap();
        let listing = list_directory(root.path(), "project-1", "").unwrap();
        assert_eq!(listing.project_id, "project-1");
        assert_eq!(listing.path, "");
        assert_eq!(listing.parent_path, None);
        assert_eq!(
            listing
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            vec!["Sources", "alpha.txt", "README.md"]
        );
        assert!(listing
            .entries
            .iter()
            .all(|entry| !entry.path.starts_with('/')));
    }

    #[test]
    fn rejects_absolute_parent_and_nonportable_paths() {
        for path in [
            "/tmp",
            "../secret",
            "folder/../secret",
            "C:/secret",
            "bad\\path",
        ] {
            assert!(normalized_relative_path(path).is_err(), "accepted {path}");
        }
        assert_eq!(normalized_relative_path("").unwrap(), "");
        assert_eq!(
            normalized_relative_path("Sources/App").unwrap(),
            "Sources/App"
        );
        assert_eq!(parent_path("Sources/App"), Some("Sources".into()));
        assert_eq!(parent_path("Sources"), Some(String::new()));
    }

    #[cfg(unix)]
    #[test]
    fn omits_symbolic_links_instead_of_exposing_external_targets() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::write(external.path().join("secret.txt"), "secret").unwrap();
        symlink(external.path(), root.path().join("external")).unwrap();
        let listing = list_directory(root.path(), "project-1", "").unwrap();
        assert!(listing.entries.is_empty());
    }
}

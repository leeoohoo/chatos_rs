use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use crate::{
    app_owned_export::replace_bytes_preserving_permissions,
    host_lifecycle::validate_owner,
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePriority,
    },
    platform_execution_context::resolve_project_root,
};

const MAXIMUM_DIRECTORY_ENTRIES: usize = 5_000;
const MAXIMUM_TEXT_PREVIEW_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_IMAGE_PREVIEW_BYTES: u64 = 25 * 1_024 * 1_024;
const MAXIMUM_SEARCH_FILE_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_SEARCH_VISITS: usize = 20_000;
const MAXIMUM_SEARCH_RESULTS: usize = 500;
const SEARCH_DURATION: Duration = Duration::from_secs(3);
const BINARY_SAMPLE_BYTES: usize = 8_000;
const MAXIMUM_LINE_PREVIEW_CHARACTERS: usize = 2_000;

const IGNORED_SEARCH_DIRECTORIES: &[&str] = &[
    ".git",
    ".build",
    ".cache",
    ".next",
    ".idea",
    ".vscode",
    "node_modules",
    "DerivedData",
    "Pods",
    "target",
    "dist",
    "build",
    "vendor",
];
const PREVIEWABLE_IMAGE_EXTENSIONS: &[&str] = &[
    "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg", "png", "svg", "tif", "tiff", "webp",
];

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileContent {
    project_id: String,
    path: String,
    name: String,
    content_type: String,
    content_encoding: &'static str,
    is_binary: bool,
    is_image: bool,
    is_writable: bool,
    size: u64,
    modified_at_unix_ms: Option<i64>,
    revision: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntrySearchResult {
    project_id: String,
    query: String,
    matches: Vec<ProjectFileEntry>,
    visited_directories: usize,
    visited_entries: usize,
    is_truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContentMatch {
    path: String,
    line: usize,
    column: usize,
    text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContentSearchResult {
    project_id: String,
    query: String,
    matches: Vec<ProjectContentMatch>,
    scanned_files: usize,
    visited_entries: usize,
    is_truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntryMutationResult {
    project_id: String,
    path: String,
    parent_path: String,
    listing: Option<ProjectDirectoryListing>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileWriteResult {
    project_id: String,
    path: String,
    file: Option<ProjectFileContent>,
}

#[tauri::command]
pub async fn load_project_directory(
    app: AppHandle,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    project_id: String,
    path: String,
    _force_refresh: bool,
) -> Result<ProjectDirectoryListing, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let relative_path = normalized_relative_path(&path, true)?;
    let app_data_directory = app.path().app_data_dir().map_err(|_| directory_error())?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id)?;
        list_directory(&root, &project_id, &relative_path)
    })
    .await
    .map_err(|_| "项目目录读取任务异常退出。".to_owned())?
}

#[tauri::command]
pub async fn read_project_file(
    app: AppHandle,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    project_id: String,
    path: String,
) -> Result<ProjectFileContent, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let relative_path = normalized_relative_path(&path, false)?;
    let app_data_directory = app.path().app_data_dir().map_err(|_| file_error())?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id)?;
        read_file(&root, &project_id, &relative_path)
    })
    .await
    .map_err(|_| "项目文件读取任务异常退出。".to_owned())?
}

#[tauri::command]
pub async fn search_project_entries(
    app: AppHandle,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    project_id: String,
    path: String,
    query: String,
    limit: usize,
) -> Result<ProjectEntrySearchResult, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let relative_path = normalized_relative_path(&path, true)?;
    let query = normalized_query(query)?;
    let limit = limit.clamp(1, MAXIMUM_SEARCH_RESULTS);
    let app_data_directory = app.path().app_data_dir().map_err(|_| search_error())?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id)?;
        search_entries(&root, &project_id, &relative_path, &query, limit)
    })
    .await
    .map_err(|_| "项目文件名搜索任务异常退出。".to_owned())?
}

#[tauri::command]
pub async fn search_project_content(
    app: AppHandle,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    project_id: String,
    path: String,
    query: String,
    limit: usize,
) -> Result<ProjectContentSearchResult, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let relative_path = normalized_relative_path(&path, true)?;
    let query = normalized_query(query)?;
    let limit = limit.clamp(1, MAXIMUM_SEARCH_RESULTS);
    let app_data_directory = app.path().app_data_dir().map_err(|_| search_error())?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id)?;
        search_content(&root, &project_id, &relative_path, &query, limit)
    })
    .await
    .map_err(|_| "项目文件内容搜索任务异常退出。".to_owned())?
}

#[tauri::command]
pub async fn write_project_file(
    app: AppHandle,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    project_id: String,
    path: String,
    expected_revision: String,
    content: String,
) -> Result<ProjectFileWriteResult, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let relative_path = normalized_relative_path(&path, false)?;
    let expected_revision = normalized_revision(&expected_revision)?;
    if content.len() > MAXIMUM_TEXT_PREVIEW_BYTES as usize {
        return Err("文件内容超过 2 MiB 写入上限。".into());
    }
    let app_data_directory = app.path().app_data_dir().map_err(|_| file_error())?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id)?;
        write_file(
            &root,
            &project_id,
            &relative_path,
            &expected_revision,
            &content,
        )
    })
    .await
    .map_err(|_| "项目文件写入任务异常退出。".to_owned())?
}

#[tauri::command]
pub async fn create_project_entry(
    app: AppHandle,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    project_id: String,
    parent_path: String,
    name: String,
    kind: String,
) -> Result<ProjectEntryMutationResult, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let parent_path = normalized_relative_path(&parent_path, true)?;
    let name = normalized_entry_name(name)?;
    let kind = mutation_kind(&kind)?;
    let app_data_directory = app.path().app_data_dir().map_err(|_| file_error())?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id)?;
        create_entry(&root, &project_id, &parent_path, &name, kind)
    })
    .await
    .map_err(|_| "项目文件创建任务异常退出。".to_owned())?
}

#[tauri::command]
pub async fn delete_project_entry(
    app: AppHandle,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    project_id: String,
    path: String,
    kind: String,
    expected_revision: Option<String>,
) -> Result<ProjectEntryMutationResult, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required("project_id", project_id)?;
    let relative_path = normalized_relative_path(&path, false)?;
    let kind = mutation_kind(&kind)?;
    let expected_revision = expected_revision
        .as_deref()
        .map(normalized_revision)
        .transpose()?;
    let app_data_directory = app.path().app_data_dir().map_err(|_| file_error())?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id)?;
        delete_entry(
            &root,
            &project_id,
            &relative_path,
            kind,
            expected_revision.as_deref(),
        )
    })
    .await
    .map_err(|_| "项目文件删除任务异常退出。".to_owned())?
}

fn resolved_project_root(
    app_data_directory: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<PathBuf, String> {
    resolve_project_root(
        &app_data_directory.join("NativeConnector"),
        owner_user_id,
        project_id,
    )
    .map(|resolved| resolved.root)
}

fn list_directory(
    project_root: &Path,
    project_id: &str,
    relative_path: &str,
) -> Result<ProjectDirectoryListing, String> {
    let directory = resolve_existing_path(project_root, relative_path, true)?;
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
        if is_link_or_reparse_point(&metadata) {
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
        entries.push(entry_from_metadata(
            relative_path,
            name,
            &metadata,
            is_directory,
        ));
    }
    sort_entries(&mut entries);
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

fn read_file(
    project_root: &Path,
    project_id: &str,
    relative_path: &str,
) -> Result<ProjectFileContent, String> {
    let file_path = resolve_existing_path(project_root, relative_path, false)?;
    let metadata = fs::symlink_metadata(&file_path).map_err(|_| file_error())?;
    if is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(file_error());
    }
    let is_image = is_previewable_image(&file_path);
    let maximum_bytes = if is_image {
        MAXIMUM_IMAGE_PREVIEW_BYTES
    } else {
        MAXIMUM_TEXT_PREVIEW_BYTES
    };
    if metadata.len() > maximum_bytes {
        return Err(file_too_large(metadata.len(), maximum_bytes));
    }
    let bytes = read_bounded(&file_path, maximum_bytes).map_err(|_| file_error())?;
    let is_binary = bytes
        .iter()
        .take(BINARY_SAMPLE_BYTES)
        .any(|byte| *byte == 0);
    let (content_encoding, content) = if is_binary {
        ("base64", BASE64.encode(&bytes))
    } else {
        ("utf8", String::from_utf8_lossy(&bytes).into_owned())
    };
    let name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(file_error)?
        .to_owned();
    let content_type = mime_guess::from_path(&file_path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_owned();
    Ok(ProjectFileContent {
        project_id: project_id.to_owned(),
        path: relative_path.to_owned(),
        name,
        content_type,
        content_encoding,
        is_binary,
        is_image,
        is_writable: !metadata.permissions().readonly(),
        size: metadata.len(),
        modified_at_unix_ms: metadata.modified().ok().and_then(unix_milliseconds),
        revision: content_revision(&bytes),
        content,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MutationKind {
    File,
    Directory,
}

fn write_file(
    project_root: &Path,
    project_id: &str,
    relative_path: &str,
    expected_revision: &str,
    content: &str,
) -> Result<ProjectFileWriteResult, String> {
    let file_path = resolve_existing_path(project_root, relative_path, false)?;
    let metadata = fs::symlink_metadata(&file_path).map_err(|_| file_error())?;
    if is_link_or_reparse_point(&metadata)
        || !metadata.is_file()
        || metadata.permissions().readonly()
    {
        return Err("项目文件不可写。".into());
    }
    let maximum = if is_previewable_image(&file_path) {
        MAXIMUM_IMAGE_PREVIEW_BYTES
    } else {
        MAXIMUM_TEXT_PREVIEW_BYTES
    };
    let current = read_bounded(&file_path, maximum).map_err(|_| file_error())?;
    if content_revision(&current) != expected_revision {
        return Err("文件已被其他程序修改，请重新加载后再保存。".into());
    }
    replace_bytes_preserving_permissions(
        &file_path,
        ".chatos-project-file-",
        content.as_bytes(),
        metadata.permissions(),
    )
    .map_err(|_| file_error())?;
    Ok(ProjectFileWriteResult {
        project_id: project_id.to_owned(),
        path: relative_path.to_owned(),
        file: read_file(project_root, project_id, relative_path).ok(),
    })
}

fn create_entry(
    project_root: &Path,
    project_id: &str,
    parent_path: &str,
    name: &str,
    kind: MutationKind,
) -> Result<ProjectEntryMutationResult, String> {
    let parent = resolve_existing_path(project_root, parent_path, true)?;
    let target = parent.join(name);
    match kind {
        MutationKind::File => {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|_| "同名文件或目录已存在。".to_owned())?;
        }
        MutationKind::Directory => {
            fs::create_dir(&target).map_err(|_| "同名文件或目录已存在。".to_owned())?;
        }
    }
    Ok(ProjectEntryMutationResult {
        project_id: project_id.to_owned(),
        path: child_path(parent_path, name),
        parent_path: parent_path.to_owned(),
        listing: list_directory(project_root, project_id, parent_path).ok(),
    })
}

fn delete_entry(
    project_root: &Path,
    project_id: &str,
    relative_path: &str,
    kind: MutationKind,
    expected_revision: Option<&str>,
) -> Result<ProjectEntryMutationResult, String> {
    let target =
        resolve_existing_path(project_root, relative_path, kind == MutationKind::Directory)?;
    let metadata = fs::symlink_metadata(&target).map_err(|_| file_error())?;
    if is_link_or_reparse_point(&metadata) {
        return Err(invalid_path());
    }
    match kind {
        MutationKind::File => {
            let expected_revision =
                expected_revision.ok_or_else(|| "删除文件缺少内容版本。".to_owned())?;
            let maximum = if is_previewable_image(&target) {
                MAXIMUM_IMAGE_PREVIEW_BYTES
            } else {
                MAXIMUM_TEXT_PREVIEW_BYTES
            };
            let current = read_bounded(&target, maximum).map_err(|_| file_error())?;
            if content_revision(&current) != expected_revision {
                return Err("文件已被其他程序修改，请重新加载后再删除。".into());
            }
            fs::remove_file(&target).map_err(|_| "无法删除项目文件。".to_owned())?;
        }
        MutationKind::Directory => {
            if expected_revision.is_some() {
                return Err("目录删除不能携带文件内容版本。".into());
            }
            fs::remove_dir_all(&target).map_err(|_| "无法删除项目目录。".to_owned())?;
        }
    }
    let parent = parent_path(relative_path).ok_or_else(invalid_path)?;
    Ok(ProjectEntryMutationResult {
        project_id: project_id.to_owned(),
        path: relative_path.to_owned(),
        parent_path: parent.clone(),
        listing: list_directory(project_root, project_id, &parent).ok(),
    })
}

fn content_revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn search_entries(
    project_root: &Path,
    project_id: &str,
    relative_path: &str,
    query: &str,
    limit: usize,
) -> Result<ProjectEntrySearchResult, String> {
    let start = resolve_existing_path(project_root, relative_path, true)?;
    let deadline = Instant::now() + SEARCH_DURATION;
    let needle = search_fold(query);
    let mut stack = vec![(start, relative_path.to_owned())];
    let mut matches = Vec::new();
    let mut visited_directories = 0;
    let mut visited_entries = 0;
    let mut is_truncated = false;

    while let Some((directory, directory_relative)) = stack.pop() {
        if Instant::now() >= deadline || visited_directories >= MAXIMUM_SEARCH_VISITS {
            is_truncated = true;
            break;
        }
        visited_directories += 1;
        let remaining_visits = MAXIMUM_SEARCH_VISITS - visited_entries;
        if remaining_visits == 0 {
            is_truncated = true;
            break;
        }
        let (mut children, enumerated, directory_truncated) =
            safe_search_children(&directory, remaining_visits);
        visited_entries += enumerated;
        is_truncated |= directory_truncated;
        children.sort_by(|left, right| left.0.cmp(&right.0));
        for (name, path, metadata) in children {
            if Instant::now() >= deadline {
                is_truncated = true;
                break;
            }
            let child_relative = child_path(&directory_relative, &name);
            let is_directory = metadata.is_dir();
            if is_directory && !ignored_search_directory(&name) {
                stack.push((path, child_relative.clone()));
            }
            if search_fold(&name).contains(&needle)
                || search_fold(&child_relative).contains(&needle)
            {
                matches.push(entry_from_metadata(
                    &directory_relative,
                    name,
                    &metadata,
                    is_directory,
                ));
                if matches.len() >= limit {
                    is_truncated = true;
                    break;
                }
            }
        }
        if is_truncated {
            break;
        }
    }
    matches.sort_by(|left, right| search_fold(&left.path).cmp(&search_fold(&right.path)));
    Ok(ProjectEntrySearchResult {
        project_id: project_id.to_owned(),
        query: query.to_owned(),
        matches,
        visited_directories,
        visited_entries,
        is_truncated,
    })
}

fn search_content(
    project_root: &Path,
    project_id: &str,
    relative_path: &str,
    query: &str,
    limit: usize,
) -> Result<ProjectContentSearchResult, String> {
    let start = resolve_existing_path(project_root, relative_path, true)?;
    let deadline = Instant::now() + SEARCH_DURATION;
    let needle = search_fold(query);
    let search_start = start.clone();
    let mut stack = vec![(start, relative_path.to_owned())];
    let mut matches = Vec::new();
    let mut scanned_files = 0;
    let mut visited_entries = 0;
    let mut is_truncated = false;
    let mut should_stop = false;

    while let Some((path, relative)) = stack.pop() {
        if Instant::now() >= deadline || visited_entries >= MAXIMUM_SEARCH_VISITS {
            is_truncated = true;
            break;
        }
        visited_entries += 1;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if !is_link_or_reparse_point(&metadata) => metadata,
            _ => continue,
        };
        if metadata.is_dir() {
            let directory_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if path != search_start && ignored_search_directory(directory_name) {
                continue;
            }
            let remaining_visits =
                MAXIMUM_SEARCH_VISITS.saturating_sub(visited_entries + stack.len());
            if remaining_visits == 0 {
                is_truncated = true;
                continue;
            }
            let (mut children, _, directory_truncated) =
                safe_search_children(&path, remaining_visits);
            is_truncated |= directory_truncated;
            children.sort_by(|left, right| right.0.cmp(&left.0));
            for (name, child, _) in children {
                stack.push((child, child_path(&relative, &name)));
            }
            continue;
        }
        if !metadata.is_file() || metadata.len() > MAXIMUM_SEARCH_FILE_BYTES {
            continue;
        }
        let bytes = match read_bounded(&path, MAXIMUM_SEARCH_FILE_BYTES) {
            Ok(bytes)
                if !bytes
                    .iter()
                    .take(BINARY_SAMPLE_BYTES)
                    .any(|byte| *byte == 0) =>
            {
                bytes
            }
            _ => continue,
        };
        scanned_files += 1;
        let content = String::from_utf8_lossy(&bytes);
        for (index, line) in content.split('\n').enumerate() {
            if Instant::now() >= deadline {
                is_truncated = true;
                should_stop = true;
                break;
            }
            let Some(column) = normalized_match_column(line, &needle) else {
                continue;
            };
            matches.push(ProjectContentMatch {
                path: relative.clone(),
                line: index + 1,
                column,
                text: line.chars().take(MAXIMUM_LINE_PREVIEW_CHARACTERS).collect(),
            });
            if matches.len() >= limit {
                is_truncated = true;
                should_stop = true;
                break;
            }
        }
        if should_stop {
            break;
        }
    }
    Ok(ProjectContentSearchResult {
        project_id: project_id.to_owned(),
        query: query.to_owned(),
        matches,
        scanned_files,
        visited_entries,
        is_truncated,
    })
}

fn safe_search_children(
    directory: &Path,
    maximum_entries: usize,
) -> (Vec<(String, PathBuf, fs::Metadata)>, usize, bool) {
    let Ok(children) = fs::read_dir(directory) else {
        return (Vec::new(), 0, false);
    };
    let mut entries = Vec::new();
    let mut enumerated = 0;
    for child in children {
        if enumerated >= maximum_entries {
            return (entries, enumerated, true);
        }
        enumerated += 1;
        let Ok(child) = child else { continue };
        let Some(name) = child.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_component(&name) {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(child.path()) else {
            continue;
        };
        if is_link_or_reparse_point(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
            continue;
        }
        entries.push((name, child.path(), metadata));
    }
    (entries, enumerated, false)
}

fn resolve_existing_path(
    project_root: &Path,
    relative_path: &str,
    expect_directory: bool,
) -> Result<PathBuf, String> {
    let root = fs::canonicalize(project_root).map_err(|_| invalid_path())?;
    let normalized = normalized_relative_path(relative_path, expect_directory)?;
    let mut candidate = root.clone();
    if !normalized.is_empty() {
        for component in normalized.split('/') {
            candidate.push(component);
            let metadata = fs::symlink_metadata(&candidate).map_err(|_| invalid_path())?;
            if is_link_or_reparse_point(&metadata) {
                return Err(invalid_path());
            }
        }
    }
    let canonical = fs::canonicalize(&candidate).map_err(|_| invalid_path())?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|_| invalid_path())?;
    if !canonical.starts_with(&root)
        || is_link_or_reparse_point(&metadata)
        || (expect_directory && !metadata.is_dir())
        || (!expect_directory && !metadata.is_file())
    {
        return Err(invalid_path());
    }
    Ok(canonical)
}

fn entry_from_metadata(
    parent: &str,
    name: String,
    metadata: &fs::Metadata,
    is_directory: bool,
) -> ProjectFileEntry {
    ProjectFileEntry {
        path: child_path(parent, &name),
        name,
        is_directory,
        is_writable: !metadata.permissions().readonly(),
        size: (!is_directory).then_some(metadata.len()),
        modified_at_unix_ms: metadata.modified().ok().and_then(unix_milliseconds),
    }
}

fn sort_entries(entries: &mut [ProjectFileEntry]) {
    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| search_fold(&left.name).cmp(&search_fold(&right.name)))
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn read_bounded(path: &Path, maximum_bytes: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(maximum_bytes.min(64 * 1_024) as usize);
    File::open(path)?
        .take(maximum_bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(std::io::Error::other("file exceeds preview limit"));
    }
    Ok(bytes)
}

fn normalized_match_column(line: &str, needle: &str) -> Option<usize> {
    let mut folded = String::new();
    let mut byte_columns = Vec::new();
    for (index, character) in line.chars().enumerate() {
        let segment = search_fold(&character.to_string());
        for _ in 0..segment.len() {
            byte_columns.push(index + 1);
        }
        folded.push_str(&segment);
    }
    let byte_index = folded.find(needle)?;
    byte_columns.get(byte_index).copied()
}

fn search_fold(value: &str) -> String {
    value
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalized_relative_path(input: &str, permits_root: bool) -> Result<String, String> {
    if input.len() > 4_096
        || input.trim() != input
        || input.contains('\\')
        || input.contains(':')
        || input.chars().any(char::is_control)
    {
        return Err(invalid_path());
    }
    if input.is_empty() && permits_root {
        return Ok(String::new());
    }
    if input.is_empty()
        || input
            .split('/')
            .any(|component| !valid_component(component))
    {
        return Err(invalid_path());
    }
    Ok(input.to_owned())
}

fn normalized_query(input: String) -> Result<String, String> {
    let query = input.trim();
    if query.is_empty() || query.len() > 1_024 || query.chars().any(char::is_control) {
        Err("搜索内容无效。".into())
    } else {
        Ok(query.to_owned())
    }
}

fn normalized_revision(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err("文件内容版本无效。".into())
    }
}

fn normalized_entry_name(value: String) -> Result<String, String> {
    let value = value.trim();
    if valid_component(value) {
        Ok(value.to_owned())
    } else {
        Err("项目文件名称无效。".into())
    }
}

fn mutation_kind(value: &str) -> Result<MutationKind, String> {
    match value {
        "file" => Ok(MutationKind::File),
        "directory" => Ok(MutationKind::Directory),
        _ => Err("项目文件类型无效。".into()),
    }
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

fn child_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

fn ignored_search_directory(name: &str) -> bool {
    IGNORED_SEARCH_DIRECTORIES.contains(&name)
}

fn is_previewable_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            PREVIEWABLE_IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
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
fn file_error() -> String {
    "无法读取项目文件。".into()
}
fn search_error() -> String {
    "无法搜索项目文件。".into()
}
fn file_too_large(size: u64, maximum: u64) -> String {
    format!(
        "文件过大，无法预览（{} 字节；上限 {} 字节）。",
        size, maximum
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_direct_children_in_macos_order_without_absolute_paths() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Sources")).unwrap();
        fs::write(root.path().join("README.md"), "hello").unwrap();
        fs::write(root.path().join("alpha.txt"), "a").unwrap();
        let listing = list_directory(root.path(), "project-1", "").unwrap();
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
    fn reads_text_lossily_and_binary_as_base64_with_macos_limits() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lossy.txt"), [b'h', 0xff, b'i']).unwrap();
        fs::write(root.path().join("binary.bin"), [1, 0, 2]).unwrap();
        let text = read_file(root.path(), "project-1", "lossy.txt").unwrap();
        assert_eq!(text.content, "h\u{fffd}i");
        let binary = read_file(root.path(), "project-1", "binary.bin").unwrap();
        assert!(binary.is_binary);
        assert_eq!(BASE64.decode(binary.content).unwrap(), [1, 0, 2]);
        fs::write(
            root.path().join("too-large.txt"),
            vec![b'a'; MAXIMUM_TEXT_PREVIEW_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read_file(root.path(), "project-1", "too-large.txt").is_err());
        fs::write(
            root.path().join("large.png"),
            vec![0; MAXIMUM_TEXT_PREVIEW_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read_file(root.path(), "project-1", "large.png").is_ok());
    }

    #[test]
    fn searches_names_and_content_case_and_diacritic_insensitively() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Sources")).unwrap();
        fs::write(
            root.path().join("Sources/Résumé.txt"),
            "first\nCAFÉ needle\nlast",
        )
        .unwrap();
        let names = search_entries(root.path(), "project-1", "", "resume", 100).unwrap();
        assert_eq!(names.matches[0].path, "Sources/Résumé.txt");
        let content = search_content(root.path(), "project-1", "", "cafe", 200).unwrap();
        assert_eq!(
            (
                content.matches[0].path.as_str(),
                content.matches[0].line,
                content.matches[0].column
            ),
            ("Sources/Résumé.txt", 2, 1)
        );
    }

    #[test]
    fn search_skips_binary_large_generated_and_symlinked_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("node_modules/package")).unwrap();
        fs::write(
            root.path().join("node_modules/package/index.js"),
            "secret needle",
        )
        .unwrap();
        fs::write(root.path().join("binary.bin"), b"needle\0hidden").unwrap();
        fs::write(
            root.path().join("large.txt"),
            vec![b'a'; MAXIMUM_SEARCH_FILE_BYTES as usize + 1],
        )
        .unwrap();
        fs::write(root.path().join("local.txt"), "local needle").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.path().join("local.txt"), root.path().join("link.txt"))
            .unwrap();
        let result = search_content(root.path(), "project-1", "", "needle", 200).unwrap();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].path, "local.txt");
    }

    #[test]
    fn bounds_results_and_rejects_nonportable_paths() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..4 {
            fs::write(root.path().join(format!("needle-{index}.txt")), "needle").unwrap();
        }
        assert!(
            search_entries(root.path(), "project-1", "", "needle", 2)
                .unwrap()
                .is_truncated
        );
        assert!(
            search_content(root.path(), "project-1", "", "needle", 2)
                .unwrap()
                .is_truncated
        );
        for path in [
            "/tmp",
            "../secret",
            "folder/../secret",
            "C:/secret",
            "bad\\path",
        ] {
            assert!(
                normalized_relative_path(path, true).is_err(),
                "accepted {path}"
            );
        }
        assert_eq!(normalized_relative_path("", true).unwrap(), "");
        assert!(normalized_relative_path("", false).is_err());
    }

    #[test]
    fn mutations_require_revisions_and_never_overwrite_create_targets() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("note.txt"), "first").unwrap();
        let initial = read_file(root.path(), "project-1", "note.txt").unwrap();
        let saved = write_file(
            root.path(),
            "project-1",
            "note.txt",
            &initial.revision,
            "second",
        )
        .unwrap();
        let saved_file = saved.file.unwrap();
        assert_eq!(saved_file.content, "second");
        assert_ne!(saved_file.revision, initial.revision);

        fs::write(root.path().join("note.txt"), "external").unwrap();
        assert!(write_file(
            root.path(),
            "project-1",
            "note.txt",
            &saved_file.revision,
            "stale",
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).unwrap(),
            "external"
        );

        create_entry(root.path(), "project-1", "", "new.txt", MutationKind::File).unwrap();
        assert!(create_entry(root.path(), "project-1", "", "new.txt", MutationKind::File).is_err());
        create_entry(
            root.path(),
            "project-1",
            "",
            "Folder",
            MutationKind::Directory,
        )
        .unwrap();

        let external = read_file(root.path(), "project-1", "note.txt").unwrap();
        assert!(delete_entry(
            root.path(),
            "project-1",
            "note.txt",
            MutationKind::File,
            Some(&saved_file.revision),
        )
        .is_err());
        delete_entry(
            root.path(),
            "project-1",
            "note.txt",
            MutationKind::File,
            Some(&external.revision),
        )
        .unwrap();
        delete_entry(
            root.path(),
            "project-1",
            "Folder",
            MutationKind::Directory,
            None,
        )
        .unwrap();
        assert!(!root.path().join("note.txt").exists());
        assert!(!root.path().join("Folder").exists());
    }
}

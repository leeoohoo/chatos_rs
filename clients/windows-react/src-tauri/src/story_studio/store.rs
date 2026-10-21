#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use super::{domain::StoryProject, projection::StoryProjectSnapshot};

const MAXIMUM_PROJECT_BYTES: usize = 8 * 1_024 * 1_024;
const DEFAULT_PROJECT_PAGE_SIZE: usize = 40;
const MAXIMUM_PROJECT_PAGE_SIZE: usize = 200;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectCursor {
    pub modified_at_unix_ms: u64,
    pub record_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryProjectPage {
    pub items: Vec<StoryProjectSnapshot>,
    pub next_cursor: Option<StoryProjectCursor>,
    pub unreadable_count: usize,
}

#[derive(Clone)]
pub(super) struct StoryProjectStore {
    root: PathBuf,
    mutation_lock: Arc<Mutex<()>>,
}

impl StoryProjectStore {
    pub(super) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        validate_private_directory(&app_data_directory)?;
        let root = private_child_directory(&app_data_directory, "StoryStudio")?;
        Ok(Self {
            root,
            mutation_lock: Arc::new(Mutex::new(())),
        })
    }

    #[cfg(test)]
    pub(super) fn for_tests(app_data_directory: PathBuf) -> Result<Self, String> {
        Self::new(app_data_directory)
    }

    pub(super) fn save(&self, owner_user_id: &str, project: &StoryProject) -> Result<(), String> {
        let owner = validate_owner(owner_user_id)?;
        project.validate()?;
        let project_id = canonical_project_id(&project.id)?;
        if project.id != project_id {
            return Err(store_error());
        }
        let bytes = serde_json::to_vec(project).map_err(|_| store_error())?;
        if bytes.is_empty() || bytes.len() > MAXIMUM_PROJECT_BYTES {
            return Err(store_error());
        }

        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        let projects = self.ensure_projects_directory(owner)?;
        let project_directory = private_child_directory(&projects, &project_id)?;
        commit_project_manifest(&project_directory, &bytes)
    }

    pub(super) fn load_project(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<StoryProjectSnapshot, String> {
        let owner = validate_owner(owner_user_id)?;
        let project_id = canonical_project_id(project_id)?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(store_error)?;
        let project = load_project_manifest(&projects.join(&project_id), &project_id)?;
        Ok(StoryProjectSnapshot::from_project(&project))
    }

    pub(super) fn load_page(
        &self,
        owner_user_id: &str,
        after: Option<StoryProjectCursor>,
        requested_limit: usize,
    ) -> Result<StoryProjectPage, String> {
        let owner = validate_owner(owner_user_id)?;
        if let Some(cursor) = &after {
            validate_cursor(cursor)?;
        }
        let limit = if requested_limit == 0 {
            DEFAULT_PROJECT_PAGE_SIZE
        } else {
            requested_limit.clamp(1, MAXIMUM_PROJECT_PAGE_SIZE)
        };
        let Some(projects_directory) = self.existing_projects_directory(owner)? else {
            return Ok(StoryProjectPage {
                items: Vec::new(),
                next_cursor: None,
                unreadable_count: 0,
            });
        };

        let mut candidates = Vec::with_capacity(limit.saturating_add(1));
        for entry in fs::read_dir(&projects_directory).map_err(|_| store_error())? {
            let Ok(entry) = entry else { continue };
            let project_id = entry.file_name().to_string_lossy().into_owned();
            if canonical_project_id(&project_id).is_err() {
                continue;
            }
            let project_directory = entry.path();
            let Ok(directory_metadata) = fs::symlink_metadata(&project_directory) else {
                continue;
            };
            if !is_private_directory_metadata(&directory_metadata) {
                continue;
            }
            let manifest = project_directory.join("project.json");
            let Ok(metadata) = fs::symlink_metadata(&manifest) else {
                continue;
            };
            let cursor = StoryProjectCursor {
                modified_at_unix_ms: system_time_milliseconds(metadata.modified().ok()),
                record_id: project_id,
            };
            if after
                .as_ref()
                .is_some_and(|after| !cursor_is_after(&cursor, after))
            {
                continue;
            }
            retain_candidate(
                ProjectCandidate {
                    directory: project_directory,
                    cursor,
                },
                &mut candidates,
                limit.saturating_add(1),
            );
        }

        let has_more = candidates.len() > limit;
        let mut projects = Vec::with_capacity(limit);
        let mut unreadable_count = 0usize;
        for candidate in candidates.iter().take(limit) {
            match load_project_manifest(&candidate.directory, &candidate.cursor.record_id) {
                Ok(project) => projects.push(project),
                Err(_) => unreadable_count = unreadable_count.saturating_add(1),
            }
        }
        projects.sort_by(|left, right| {
            right
                .updated_at_unix_ms
                .cmp(&left.updated_at_unix_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        let next_cursor = has_more
            .then(|| {
                candidates
                    .get(limit.saturating_sub(1))
                    .map(|value| value.cursor.clone())
            })
            .flatten();
        Ok(StoryProjectPage {
            items: projects
                .iter()
                .map(StoryProjectSnapshot::from_project)
                .collect(),
            next_cursor,
            unreadable_count,
        })
    }

    fn ensure_projects_directory(&self, owner: &str) -> Result<PathBuf, String> {
        validate_private_directory(&self.root)?;
        let accounts = private_child_directory(&self.root, "Accounts")?;
        let account = private_child_directory(&accounts, &owner_hash(owner))?;
        private_child_directory(&account, "Projects")
    }

    fn existing_projects_directory(&self, owner: &str) -> Result<Option<PathBuf>, String> {
        validate_private_directory(&self.root)?;
        let accounts = self.root.join("Accounts");
        if !validate_optional_private_directory(&accounts)? {
            return Ok(None);
        }
        let account = accounts.join(owner_hash(owner));
        if !validate_optional_private_directory(&account)? {
            return Ok(None);
        }
        let projects = account.join("Projects");
        if !validate_optional_private_directory(&projects)? {
            return Ok(None);
        }
        Ok(Some(projects))
    }
}

#[derive(Clone)]
struct ProjectCandidate {
    directory: PathBuf,
    cursor: StoryProjectCursor,
}

fn load_project_manifest(
    directory: &Path,
    expected_project_id: &str,
) -> Result<StoryProject, String> {
    validate_private_directory(directory)?;
    let bytes = read_private_file(&directory.join("project.json"), MAXIMUM_PROJECT_BYTES)?;
    let project: StoryProject = serde_json::from_slice(&bytes).map_err(|_| store_error())?;
    if project.id != expected_project_id {
        return Err(store_error());
    }
    project.validate()?;
    Ok(project)
}

fn commit_project_manifest(directory: &Path, bytes: &[u8]) -> Result<(), String> {
    validate_private_directory(directory)?;
    let temporary = directory.join("project.json.tmp");
    let target = directory.join("project.json");
    if let Ok(metadata) = fs::symlink_metadata(&temporary) {
        if !is_private_file_metadata(&metadata) {
            return Err(store_error());
        }
        fs::remove_file(&temporary).map_err(|_| store_error())?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&target) {
        if !is_private_file_metadata(&metadata) {
            return Err(store_error());
        }
    }
    write_private_file(&temporary, bytes)?;
    if let Err(error) = replace_file(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_directory(directory)
}

fn read_private_file(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| store_error())?;
    if !is_private_file_metadata(&link_metadata)
        || link_metadata.len() == 0
        || link_metadata.len() > maximum_bytes as u64
    {
        return Err(store_error());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path).map_err(|_| store_error())?;
    let before = file.metadata().map_err(|_| store_error())?;
    if !is_private_file_metadata(&before) {
        return Err(store_error());
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take((maximum_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| store_error())?;
    let after = file.metadata().map_err(|_| store_error())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
        || bytes.len() > maximum_bytes
    {
        return Err(store_error());
    }
    Ok(bytes)
}

fn private_child_directory(parent: &Path, name: &str) -> Result<PathBuf, String> {
    validate_private_directory(parent)?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !is_private_directory_metadata(&metadata) {
                return Err(store_error());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|_| store_error())?;
        }
        Err(_) => return Err(store_error()),
    }
    set_private_directory_permissions(&path)?;
    validate_private_directory(&path)?;
    sync_directory(parent)?;
    Ok(path)
}

fn validate_private_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| store_error())?;
    if !is_private_directory_metadata(&metadata) {
        return Err(store_error());
    }
    Ok(())
}

fn validate_optional_private_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_private_directory_metadata(&metadata) => Ok(true),
        Ok(_) => Err(store_error()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(store_error()),
    }
}

fn is_private_directory_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
}

fn is_private_file_metadata(metadata: &fs::Metadata) -> bool {
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

fn set_private_directory_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| store_error())?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| store_error())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| store_error())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|_| store_error())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
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
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(store_error())
    } else {
        Ok(())
    }
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| store_error())?;
    }
    Ok(())
}

fn retain_candidate(
    candidate: ProjectCandidate,
    values: &mut Vec<ProjectCandidate>,
    capacity: usize,
) {
    let index = values
        .iter()
        .position(|value| cursor_is_newer(&candidate.cursor, &value.cursor))
        .unwrap_or(values.len());
    if index >= capacity {
        return;
    }
    values.insert(index, candidate);
    if values.len() > capacity {
        values.pop();
    }
}

fn cursor_is_newer(left: &StoryProjectCursor, right: &StoryProjectCursor) -> bool {
    left.modified_at_unix_ms > right.modified_at_unix_ms
        || (left.modified_at_unix_ms == right.modified_at_unix_ms
            && left.record_id > right.record_id)
}

fn cursor_is_after(value: &StoryProjectCursor, cursor: &StoryProjectCursor) -> bool {
    value.modified_at_unix_ms < cursor.modified_at_unix_ms
        || (value.modified_at_unix_ms == cursor.modified_at_unix_ms
            && value.record_id < cursor.record_id)
}

fn validate_cursor(cursor: &StoryProjectCursor) -> Result<(), String> {
    canonical_project_id(&cursor.record_id).map(|_| ())
}

fn canonical_project_id(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value).map_err(|_| store_error())?;
    let canonical = parsed.to_string();
    if canonical != value {
        return Err(store_error());
    }
    Ok(canonical)
}

fn validate_owner(value: &str) -> Result<&str, String> {
    let owner = value.trim();
    if owner.is_empty() || owner.len() > 1_024 {
        return Err("剧情项目缺少有效的账号上下文。".into());
    }
    Ok(owner)
}

fn owner_hash(owner_user_id: &str) -> String {
    format!("{:x}", Sha256::digest(owner_user_id.as_bytes()))
}

fn system_time_milliseconds(value: Option<std::time::SystemTime>) -> u64 {
    value
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|value| value.as_millis().try_into().ok())
        .unwrap_or_default()
}

fn store_error() -> String {
    "剧情项目文件或记录无效。".into()
}

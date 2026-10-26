use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

use super::{
    history::owner_hash,
    video_provider::{VideoGenerationResult, MAXIMUM_VIDEO_BYTES},
};

const MANIFEST_VERSION: u32 = 1;
const MAXIMUM_MANIFEST_BYTES: usize = 2 * 1_024 * 1_024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoHistoryItem {
    pub id: String,
    pub prompt: String,
    pub model_name: String,
    pub created_at_unix_ms: u64,
    pub mime_type: String,
    pub content_token: String,
    pub preview_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VideoHistoryCursor {
    pub modified_at_unix_ms: u64,
    pub record_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoHistoryPage {
    pub items: Vec<VideoHistoryItem>,
    pub next_cursor: Option<VideoHistoryCursor>,
    pub unreadable_count: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VideoManifest {
    version: u32,
    id: String,
    kind: String,
    prompt: String,
    model_name: String,
    created_at_unix_ms: u64,
    asset: VideoManifestAsset,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VideoManifestAsset {
    id: String,
    mime_type: String,
    filename: String,
    content_token: String,
}

#[derive(Clone)]
pub(super) struct MediaStudioVideoHistoryStore {
    root: PathBuf,
}

impl MediaStudioVideoHistoryStore {
    pub(super) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            root: app_data_directory.join("MediaStudio").join("VideoAccounts"),
        }
    }

    #[cfg(test)]
    fn for_tests(root: PathBuf) -> Self {
        Self { root }
    }

    pub(super) fn temporary_cleanup_root(&self) -> PathBuf {
        self.root.clone()
    }

    pub(super) fn persist_video(
        &self,
        owner_user_id: &str,
        prompt: &str,
        result: &VideoGenerationResult,
        cancelled: &AtomicBool,
    ) -> Result<VideoHistoryItem, String> {
        if cancelled.load(Ordering::Acquire)
            || result.video_data.is_empty()
            || result.video_data.len() > MAXIMUM_VIDEO_BYTES
            || !safe_video_mime_type(&result.mime_type)
        {
            return Err(video_history_error());
        }
        let record_id = Uuid::new_v4().to_string();
        let account_directory = self.root.join(owner_hash(owner_user_id));
        let record_directory = account_directory.join(&record_id);
        ensure_private_directory(&account_directory)?;
        fs::create_dir(&record_directory).map_err(|_| video_history_error())?;
        let content_token = Uuid::new_v4().simple().to_string();
        let extension = if result.mime_type == "video/quicktime" {
            "mov"
        } else {
            "mp4"
        };
        let filename = format!("{content_token}.{extension}");
        let asset_path = record_directory.join(&filename);
        if let Err(error) =
            write_private_file_cancellable(&asset_path, &result.video_data, cancelled)
        {
            let _ = fs::remove_dir_all(&record_directory);
            return Err(error);
        }
        if cancelled.load(Ordering::Acquire) {
            let _ = fs::remove_dir_all(&record_directory);
            return Err("视频生成已取消。".into());
        }
        let manifest = VideoManifest {
            version: MANIFEST_VERSION,
            id: record_id,
            kind: "video".into(),
            prompt: prompt.to_owned(),
            model_name: result.model_name.clone(),
            created_at_unix_ms: result.created_at_unix_ms,
            asset: VideoManifestAsset {
                id: result.id.clone(),
                mime_type: result.mime_type.clone(),
                filename,
                content_token,
            },
        };
        if let Err(error) = commit_manifest(&record_directory, &manifest, cancelled) {
            let _ = fs::remove_dir_all(&record_directory);
            return Err(error);
        }
        Ok(video_history_item(&manifest))
    }

    pub(super) fn load_page(
        &self,
        owner_user_id: &str,
        after: Option<VideoHistoryCursor>,
        requested_limit: usize,
    ) -> Result<VideoHistoryPage, String> {
        if let Some(cursor) = &after {
            validate_cursor(cursor)?;
        }
        let limit = requested_limit.clamp(1, 200);
        let account_directory = self.root.join(owner_hash(owner_user_id));
        if !account_directory.exists() {
            return Ok(VideoHistoryPage {
                items: Vec::new(),
                next_cursor: None,
                unreadable_count: 0,
            });
        }
        validate_private_directory(&account_directory)?;
        let mut candidates = Vec::with_capacity(limit.saturating_add(1));
        for entry in fs::read_dir(&account_directory).map_err(|_| video_history_error())? {
            let Ok(entry) = entry else { continue };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let record_id = entry.file_name().to_string_lossy().into_owned();
            if !valid_uuid(&record_id) {
                continue;
            }
            let manifest_path = entry.path().join("record.json");
            let Ok(metadata) = fs::symlink_metadata(&manifest_path) else {
                continue;
            };
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() == 0
                || metadata.len() > MAXIMUM_MANIFEST_BYTES as u64
            {
                continue;
            }
            let cursor = VideoHistoryCursor {
                modified_at_unix_ms: system_time_milliseconds(metadata.modified().ok()),
                record_id,
            };
            if after
                .as_ref()
                .is_some_and(|after| !cursor_is_after(&cursor, after))
            {
                continue;
            }
            retain_candidate(
                HistoryCandidate {
                    directory: entry.path(),
                    cursor,
                },
                &mut candidates,
                limit.saturating_add(1),
            );
        }
        let has_more = candidates.len() > limit;
        let mut items = Vec::with_capacity(limit);
        let mut unreadable_count = 0usize;
        for candidate in candidates.iter().take(limit) {
            match load_manifest(&candidate.directory, &candidate.cursor.record_id) {
                Ok(manifest) => items.push(video_history_item(&manifest)),
                Err(_) => unreadable_count = unreadable_count.saturating_add(1),
            }
        }
        items.sort_by(|left, right| {
            right
                .created_at_unix_ms
                .cmp(&left.created_at_unix_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        let next_cursor = has_more
            .then(|| {
                candidates
                    .get(limit.saturating_sub(1))
                    .map(|value| value.cursor.clone())
            })
            .flatten();
        Ok(VideoHistoryPage {
            items,
            next_cursor,
            unreadable_count,
        })
    }

    pub(super) fn read_asset(
        &self,
        owner_hash_value: &str,
        record_id: &str,
        content_token: &str,
    ) -> Result<(String, Vec<u8>), String> {
        if !valid_owner_hash(owner_hash_value)
            || !valid_uuid(record_id)
            || !valid_content_token(content_token)
        {
            return Err(video_history_error());
        }
        let account_directory = self.root.join(owner_hash_value);
        validate_private_directory(&account_directory)?;
        let record_directory = account_directory.join(record_id);
        validate_private_directory(&record_directory)?;
        let manifest = load_manifest(&record_directory, record_id)?;
        if manifest.asset.content_token != content_token {
            return Err(video_history_error());
        }
        let bytes = read_private_file(
            &record_directory.join(&manifest.asset.filename),
            MAXIMUM_VIDEO_BYTES,
        )?;
        Ok((manifest.asset.mime_type, bytes))
    }
}

#[derive(Clone)]
struct HistoryCandidate {
    directory: PathBuf,
    cursor: VideoHistoryCursor,
}

fn retain_candidate(
    candidate: HistoryCandidate,
    values: &mut Vec<HistoryCandidate>,
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

fn cursor_is_newer(left: &VideoHistoryCursor, right: &VideoHistoryCursor) -> bool {
    left.modified_at_unix_ms > right.modified_at_unix_ms
        || (left.modified_at_unix_ms == right.modified_at_unix_ms
            && left.record_id > right.record_id)
}

fn cursor_is_after(value: &VideoHistoryCursor, cursor: &VideoHistoryCursor) -> bool {
    value.modified_at_unix_ms < cursor.modified_at_unix_ms
        || (value.modified_at_unix_ms == cursor.modified_at_unix_ms
            && value.record_id < cursor.record_id)
}

fn validate_cursor(cursor: &VideoHistoryCursor) -> Result<(), String> {
    if cursor.modified_at_unix_ms == 0 || !valid_uuid(&cursor.record_id) {
        return Err("视频历史游标无效。".into());
    }
    Ok(())
}

fn load_manifest(directory: &Path, record_id: &str) -> Result<VideoManifest, String> {
    let bytes = read_private_file(&directory.join("record.json"), MAXIMUM_MANIFEST_BYTES)?;
    let manifest: VideoManifest =
        serde_json::from_slice(&bytes).map_err(|_| video_history_error())?;
    if manifest.version != MANIFEST_VERSION
        || manifest.id != record_id
        || manifest.kind != "video"
        || manifest.prompt.len() > 100_000
        || manifest.model_name.trim().is_empty()
        || manifest.model_name.len() > 1_024
        || manifest.asset.id.trim().is_empty()
        || manifest.asset.id.len() > 1_024
        || !safe_video_mime_type(&manifest.asset.mime_type)
        || !valid_content_token(&manifest.asset.content_token)
        || !valid_asset_filename(
            &manifest.asset.filename,
            &manifest.asset.content_token,
            &manifest.asset.mime_type,
        )
    {
        return Err(video_history_error());
    }
    let metadata = fs::symlink_metadata(directory.join(&manifest.asset.filename))
        .map_err(|_| video_history_error())?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAXIMUM_VIDEO_BYTES as u64
    {
        return Err(video_history_error());
    }
    Ok(manifest)
}

fn video_history_item(manifest: &VideoManifest) -> VideoHistoryItem {
    VideoHistoryItem {
        id: manifest.id.clone(),
        prompt: manifest.prompt.clone(),
        model_name: manifest.model_name.clone(),
        created_at_unix_ms: manifest.created_at_unix_ms,
        mime_type: manifest.asset.mime_type.clone(),
        content_token: manifest.asset.content_token.clone(),
        preview_url: None,
    }
}

fn commit_manifest(
    directory: &Path,
    manifest: &VideoManifest,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("视频生成已取消。".into());
    }
    let bytes = serde_json::to_vec(manifest).map_err(|_| video_history_error())?;
    if bytes.len() > MAXIMUM_MANIFEST_BYTES {
        return Err(video_history_error());
    }
    let temporary = directory.join("record.json.tmp");
    let target = directory.join("record.json");
    write_private_file_cancellable(&temporary, &bytes, cancelled)?;
    if cancelled.load(Ordering::Acquire) {
        let _ = fs::remove_file(&temporary);
        return Err("视频生成已取消。".into());
    }
    fs::rename(&temporary, &target).map_err(|_| video_history_error())?;
    if let Ok(record) = File::open(directory) {
        let _ = record.sync_all();
    }
    Ok(())
}

fn write_private_file_cancellable(
    path: &Path,
    bytes: &[u8],
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| video_history_error())?;
    for chunk in bytes.chunks(1_024 * 1_024) {
        if cancelled.load(Ordering::Acquire) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err("视频生成已取消。".into());
        }
        file.write_all(chunk).map_err(|_| video_history_error())?;
    }
    file.sync_all().map_err(|_| video_history_error())
}

fn read_private_file(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| video_history_error())?;
    if !link_metadata.file_type().is_file()
        || link_metadata.file_type().is_symlink()
        || link_metadata.len() == 0
        || link_metadata.len() > maximum_bytes as u64
    {
        return Err(video_history_error());
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
    let mut file = options.open(path).map_err(|_| video_history_error())?;
    let before = file.metadata().map_err(|_| video_history_error())?;
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take((maximum_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| video_history_error())?;
    let after = file.metadata().map_err(|_| video_history_error())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
        || bytes.len() > maximum_bytes
    {
        return Err(video_history_error());
    }
    Ok(bytes)
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|_| video_history_error())?;
    let metadata = fs::symlink_metadata(path).map_err(|_| video_history_error())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(video_history_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| video_history_error())?;
    }
    Ok(())
}

fn validate_private_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| video_history_error())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(video_history_error());
    }
    Ok(())
}

fn safe_video_mime_type(value: &str) -> bool {
    matches!(value, "video/mp4" | "video/quicktime")
}

fn valid_asset_filename(filename: &str, content_token: &str, mime_type: &str) -> bool {
    let extension = if mime_type == "video/quicktime" {
        ".mov"
    } else {
        ".mp4"
    };
    Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value == filename && value == format!("{content_token}{extension}"))
}

fn valid_owner_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.to_string() == value)
        .unwrap_or(false)
}

fn valid_content_token(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn system_time_milliseconds(value: Option<std::time::SystemTime>) -> u64 {
    value
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|value| value.as_millis().try_into().ok())
        .unwrap_or_default()
}

fn video_history_error() -> String {
    "视频文件或记录无效。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn result(id: &str) -> VideoGenerationResult {
        VideoGenerationResult {
            id: id.into(),
            model_config_id: "video-model".into(),
            model_name: "MiniMax-H3".into(),
            created_at_unix_ms: 1_700_000_000_000,
            mime_type: "video/mp4".into(),
            video_data: format!("video-{id}").into_bytes(),
        }
    }

    #[test]
    fn persisted_video_uses_relative_manifest_and_owner_scope() {
        let root = tempdir().unwrap();
        let store = MediaStudioVideoHistoryStore::for_tests(root.path().to_path_buf());
        let item = store
            .persist_video("alice", "clip", &result("one"), &AtomicBool::new(false))
            .unwrap();
        let encoded = serde_json::to_string(&item).unwrap();
        assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
        let page = store.load_page("alice", None, 10).unwrap();
        assert_eq!(page.items, vec![item.clone()]);
        assert!(store.load_page("bob", None, 10).unwrap().items.is_empty());
        let (mime_type, bytes) = store
            .read_asset(&owner_hash("alice"), &item.id, &item.content_token)
            .unwrap();
        assert_eq!(mime_type, "video/mp4");
        assert_eq!(bytes, b"video-one");
    }

    #[test]
    fn cancellation_before_commit_leaves_no_history_record() {
        let root = tempdir().unwrap();
        let store = MediaStudioVideoHistoryStore::for_tests(root.path().to_path_buf());
        let cancelled = AtomicBool::new(true);
        assert!(store
            .persist_video("alice", "clip", &result("late"), &cancelled)
            .is_err());
        assert!(store.load_page("alice", None, 10).unwrap().items.is_empty());
    }

    #[test]
    fn pagination_is_newest_first_and_damaged_records_are_isolated() {
        let root = tempdir().unwrap();
        let store = MediaStudioVideoHistoryStore::for_tests(root.path().to_path_buf());
        let first = store
            .persist_video("alice", "one", &result("one"), &AtomicBool::new(false))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = store
            .persist_video("alice", "two", &result("two"), &AtomicBool::new(false))
            .unwrap();
        let page = store.load_page("alice", None, 1).unwrap();
        assert_eq!(page.items[0].id, second.id);
        let next = store.load_page("alice", page.next_cursor, 1).unwrap();
        assert_eq!(next.items[0].id, first.id);

        let damaged = store
            .root
            .join(owner_hash("alice"))
            .join(&first.id)
            .join("record.json");
        fs::write(damaged, b"not-json").unwrap();
        let page = store.load_page("alice", None, 10).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.unreadable_count, 1);
    }
}

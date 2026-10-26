use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use super::provider::{GeneratedImageAsset, ImageGenerationResult};

const MANIFEST_VERSION: u32 = 1;
const MAXIMUM_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_MANIFEST_BYTES: usize = 2 * 1_024 * 1_024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageHistoryItem {
    pub id: String,
    pub prompt: String,
    pub model_name: String,
    pub created_at_unix_ms: u64,
    pub images: Vec<ImageHistoryAsset>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageHistoryAsset {
    pub id: String,
    pub mime_type: String,
    pub content_token: String,
    pub revised_prompt: Option<String>,
    pub preview_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImageHistoryCursor {
    pub modified_at_unix_ms: u64,
    pub record_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageHistoryPage {
    pub items: Vec<ImageHistoryItem>,
    pub next_cursor: Option<ImageHistoryCursor>,
    pub unreadable_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub(crate) enum ImageGenerationOutcome {
    Saved {
        item: ImageHistoryItem,
    },
    GeneratedWithoutPersistence {
        result: ImageGenerationResult,
        persistence_error_message: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryManifest {
    version: u32,
    id: String,
    kind: String,
    prompt: String,
    model_name: String,
    created_at_unix_ms: u64,
    assets: Vec<HistoryManifestAsset>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryManifestAsset {
    id: String,
    mime_type: String,
    filename: String,
    content_token: String,
    revised_prompt: Option<String>,
}

#[derive(Clone)]
pub(crate) struct MediaStudioHistoryStore {
    root: PathBuf,
    client: Client,
}

impl MediaStudioHistoryStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|_| "无法初始化图片历史网络客户端。".to_owned())?;
        Ok(Self {
            root: app_data_directory.join("MediaStudio"),
            client,
        })
    }

    #[cfg(test)]
    pub(super) fn for_tests(root: PathBuf) -> Self {
        Self {
            root,
            client: Client::new(),
        }
    }

    pub(super) fn temporary_cleanup_root(&self) -> PathBuf {
        self.root.join("Accounts")
    }

    pub(crate) async fn persist_image(
        &self,
        owner_user_id: &str,
        prompt: &str,
        result: &ImageGenerationResult,
    ) -> Result<ImageHistoryItem, String> {
        if result.images.is_empty() {
            return Err(history_error());
        }
        let record_id = Uuid::new_v4().to_string();
        let account_directory = self.root.join("Accounts").join(owner_hash(owner_user_id));
        let record_directory = account_directory.join(&record_id);
        ensure_private_directory(&account_directory)?;
        fs::create_dir(&record_directory).map_err(|_| history_error())?;
        let persisted = self.persist_assets(&record_directory, &result.images).await;
        let manifest_assets = match persisted {
            Ok(assets) => assets,
            Err(error) => {
                let _ = fs::remove_dir_all(&record_directory);
                return Err(error);
            }
        };
        let manifest = HistoryManifest {
            version: MANIFEST_VERSION,
            id: record_id.clone(),
            kind: "image".into(),
            prompt: prompt.to_owned(),
            model_name: result.model_name.clone(),
            created_at_unix_ms: result.created_at_unix_ms,
            assets: manifest_assets,
        };
        if let Err(error) = commit_manifest(&record_directory, &manifest) {
            let _ = fs::remove_dir_all(&record_directory);
            return Err(error);
        }
        Ok(history_item(&manifest))
    }

    pub(crate) fn load_image_page(
        &self,
        owner_user_id: &str,
        after: Option<ImageHistoryCursor>,
        requested_limit: usize,
    ) -> Result<ImageHistoryPage, String> {
        if let Some(cursor) = &after {
            validate_cursor(cursor)?;
        }
        let limit = requested_limit.clamp(1, 200);
        let account_directory = self.root.join("Accounts").join(owner_hash(owner_user_id));
        if !account_directory.exists() {
            return Ok(ImageHistoryPage {
                items: Vec::new(),
                next_cursor: None,
                unreadable_count: 0,
            });
        }
        validate_private_directory(&account_directory)?;
        let mut candidates = Vec::with_capacity(limit.saturating_add(1));
        let entries = fs::read_dir(&account_directory).map_err(|_| history_error())?;
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let record_id = entry.file_name().to_string_lossy().into_owned();
            if !valid_record_id(&record_id) {
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
            let cursor = ImageHistoryCursor {
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
        let page_candidates = candidates.iter().take(limit);
        let mut items = Vec::with_capacity(limit);
        let mut unreadable_count = 0usize;
        for candidate in page_candidates {
            match load_manifest(&candidate.directory, &candidate.cursor.record_id) {
                Ok(manifest) => items.push(history_item(&manifest)),
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
        Ok(ImageHistoryPage {
            items,
            next_cursor,
            unreadable_count,
        })
    }

    pub(crate) fn read_history_asset(
        &self,
        owner_hash: &str,
        record_id: &str,
        content_token: &str,
    ) -> Result<(String, Vec<u8>), String> {
        if !valid_owner_hash(owner_hash)
            || !valid_record_id(record_id)
            || !valid_content_token(content_token)
        {
            return Err(history_error());
        }
        let account_directory = self.root.join("Accounts").join(owner_hash);
        validate_private_directory(&account_directory)?;
        let record_directory = account_directory.join(record_id);
        validate_private_directory(&record_directory)?;
        let manifest = load_manifest(&record_directory, record_id)?;
        let asset = manifest
            .assets
            .iter()
            .find(|asset| asset.content_token == content_token)
            .ok_or_else(history_error)?;
        let path = record_directory.join(&asset.filename);
        let bytes = read_private_file(&path, MAXIMUM_IMAGE_BYTES)?;
        Ok((asset.mime_type.clone(), bytes))
    }

    async fn persist_assets(
        &self,
        record_directory: &Path,
        assets: &[GeneratedImageAsset],
    ) -> Result<Vec<HistoryManifestAsset>, String> {
        let mut persisted = Vec::with_capacity(assets.len());
        for asset in assets {
            let bytes = self.asset_bytes(asset).await?;
            let extension = image_extension(&asset.mime_type);
            let content_token = Uuid::new_v4().simple().to_string();
            let filename = format!("{content_token}.{extension}");
            write_private_file(&record_directory.join(&filename), &bytes)?;
            persisted.push(HistoryManifestAsset {
                id: asset.id.clone(),
                mime_type: asset.mime_type.clone(),
                filename,
                content_token,
                revised_prompt: asset.revised_prompt.clone(),
            });
        }
        Ok(persisted)
    }

    pub(super) async fn asset_bytes(&self, asset: &GeneratedImageAsset) -> Result<Vec<u8>, String> {
        let bytes = if let Some(base64_data) = asset.base64_data.as_deref() {
            BASE64.decode(base64_data).map_err(|_| history_error())?
        } else if let Some(url) = asset.url.as_deref() {
            let url = trusted_https_url(url)?;
            let response = self
                .client
                .get(url)
                .header(reqwest::header::ACCEPT, "image/*")
                .send()
                .await
                .map_err(|_| "未能下载生成的图片到本机。".to_owned())?;
            if !response.status().is_success() {
                return Err("未能下载生成的图片到本机。".into());
            }
            read_bounded_response(response, MAXIMUM_IMAGE_BYTES).await?
        } else {
            return Err(history_error());
        };
        if bytes.is_empty() || bytes.len() > MAXIMUM_IMAGE_BYTES {
            return Err(history_error());
        }
        Ok(bytes)
    }
}

async fn read_bounded_response(
    mut response: reqwest::Response,
    maximum_bytes: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum_bytes as u64)
    {
        return Err("未能下载生成的图片到本机。".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "未能下载生成的图片到本机。".to_owned())?
    {
        if body.len().saturating_add(chunk.len()) > maximum_bytes {
            return Err("未能下载生成的图片到本机。".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn history_item(manifest: &HistoryManifest) -> ImageHistoryItem {
    ImageHistoryItem {
        id: manifest.id.clone(),
        prompt: manifest.prompt.clone(),
        model_name: manifest.model_name.clone(),
        created_at_unix_ms: manifest.created_at_unix_ms,
        images: manifest
            .assets
            .iter()
            .map(|asset| ImageHistoryAsset {
                id: asset.id.clone(),
                mime_type: asset.mime_type.clone(),
                content_token: asset.content_token.clone(),
                revised_prompt: asset.revised_prompt.clone(),
                preview_url: None,
            })
            .collect(),
    }
}

#[derive(Clone)]
struct HistoryCandidate {
    directory: PathBuf,
    cursor: ImageHistoryCursor,
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

fn cursor_is_newer(left: &ImageHistoryCursor, right: &ImageHistoryCursor) -> bool {
    left.modified_at_unix_ms > right.modified_at_unix_ms
        || (left.modified_at_unix_ms == right.modified_at_unix_ms
            && left.record_id > right.record_id)
}

fn cursor_is_after(value: &ImageHistoryCursor, cursor: &ImageHistoryCursor) -> bool {
    value.modified_at_unix_ms < cursor.modified_at_unix_ms
        || (value.modified_at_unix_ms == cursor.modified_at_unix_ms
            && value.record_id < cursor.record_id)
}

fn validate_cursor(cursor: &ImageHistoryCursor) -> Result<(), String> {
    if cursor.modified_at_unix_ms == 0 || !valid_record_id(&cursor.record_id) {
        return Err("图片历史游标无效。".into());
    }
    Ok(())
}

fn load_manifest(directory: &Path, record_id: &str) -> Result<HistoryManifest, String> {
    let bytes = read_private_file(&directory.join("record.json"), MAXIMUM_MANIFEST_BYTES)?;
    let manifest: HistoryManifest = serde_json::from_slice(&bytes).map_err(|_| history_error())?;
    if manifest.version != MANIFEST_VERSION
        || manifest.id != record_id
        || manifest.kind != "image"
        || manifest.prompt.len() > 100_000
        || manifest.model_name.trim().is_empty()
        || manifest.model_name.len() > 1_024
        || manifest.assets.is_empty()
        || manifest.assets.len() > 10
    {
        return Err(history_error());
    }
    for asset in &manifest.assets {
        if asset.id.trim().is_empty()
            || asset.id.len() > 1_024
            || !safe_image_mime_type(&asset.mime_type)
            || !valid_content_token(&asset.content_token)
            || !valid_asset_filename(&asset.filename, &asset.content_token)
            || asset
                .revised_prompt
                .as_ref()
                .is_some_and(|value| value.len() > 100_000)
        {
            return Err(history_error());
        }
        let metadata =
            fs::symlink_metadata(directory.join(&asset.filename)).map_err(|_| history_error())?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() == 0
            || metadata.len() > MAXIMUM_IMAGE_BYTES as u64
        {
            return Err(history_error());
        }
    }
    Ok(manifest)
}

fn read_private_file(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| history_error())?;
    if !link_metadata.file_type().is_file()
        || link_metadata.file_type().is_symlink()
        || link_metadata.len() == 0
        || link_metadata.len() > maximum_bytes as u64
    {
        return Err(history_error());
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
    let mut file = options.open(path).map_err(|_| history_error())?;
    let before = file.metadata().map_err(|_| history_error())?;
    let mut bytes = Vec::with_capacity(before.len() as usize);
    use std::io::Read;
    (&mut file)
        .take((maximum_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| history_error())?;
    let after = file.metadata().map_err(|_| history_error())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
        || bytes.len() > maximum_bytes
    {
        return Err(history_error());
    }
    Ok(bytes)
}

fn validate_private_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| history_error())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(history_error());
    }
    Ok(())
}

fn valid_owner_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_record_id(value: &str) -> bool {
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

fn safe_image_mime_type(value: &str) -> bool {
    matches!(
        value,
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    )
}

fn valid_asset_filename(filename: &str, content_token: &str) -> bool {
    Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value == filename && value.starts_with(content_token))
        && matches!(
            filename.strip_prefix(content_token),
            Some(".png" | ".jpg" | ".webp" | ".gif")
        )
}

fn system_time_milliseconds(value: Option<std::time::SystemTime>) -> u64 {
    value
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|value| value.as_millis().try_into().ok())
        .unwrap_or_default()
}

fn commit_manifest(directory: &Path, manifest: &HistoryManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec(manifest).map_err(|_| history_error())?;
    if bytes.len() > MAXIMUM_MANIFEST_BYTES {
        return Err(history_error());
    }
    let temporary = directory.join("record.json.tmp");
    let target = directory.join("record.json");
    write_private_file(&temporary, &bytes)?;
    fs::rename(&temporary, &target).map_err(|_| history_error())?;
    if let Ok(record) = File::open(directory) {
        let _ = record.sync_all();
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|_| history_error())?;
    let metadata = fs::symlink_metadata(path).map_err(|_| history_error())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(history_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| history_error())?;
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
    let mut file = options.open(path).map_err(|_| history_error())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| history_error())
}

fn trusted_https_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| history_error())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(history_error());
    }
    Ok(url)
}

fn image_extension(mime_type: &str) -> &'static str {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    }
}

pub(super) fn owner_hash(owner_user_id: &str) -> String {
    format!("{:x}", Sha256::digest(owner_user_id.as_bytes()))
}

fn history_error() -> String {
    "创作文件或记录无效。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn result() -> ImageGenerationResult {
        ImageGenerationResult {
            id: "provider-result".into(),
            model_config_id: "image-model".into(),
            model_name: "gpt-image-1".into(),
            created_at_unix_ms: 1_700_000_000_000,
            images: vec![GeneratedImageAsset {
                id: "asset-1".into(),
                mime_type: "image/png".into(),
                base64_data: Some(BASE64.encode(b"image-bytes")),
                url: None,
                revised_prompt: Some("revised".into()),
            }],
            client_request_id: "request-1".into(),
        }
    }

    #[tokio::test]
    async fn image_history_is_atomic_relative_and_owner_scoped() {
        let root = tempdir().unwrap();
        let store = MediaStudioHistoryStore::for_tests(root.path().to_path_buf());
        let item = store
            .persist_image("alice", "quiet lake", &result())
            .await
            .unwrap();
        assert_eq!(item.prompt, "quiet lake");
        assert_eq!(item.images[0].id, "asset-1");
        let alice = root.path().join("Accounts").join(owner_hash("alice"));
        let bob = root.path().join("Accounts").join(owner_hash("bob"));
        assert!(alice.exists());
        assert!(!bob.exists());
        let record = alice.join(&item.id);
        let manifest = fs::read(record.join("record.json")).unwrap();
        let encoded = String::from_utf8(manifest).unwrap();
        assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
        assert!(!encoded.contains("image-bytes"));
        assert!(encoded.contains(&item.images[0].content_token));
        assert_eq!(
            fs::read(record.join(format!("{}.png", item.images[0].content_token))).unwrap(),
            b"image-bytes"
        );
    }

    #[tokio::test]
    async fn history_pages_are_bounded_cursor_stable_and_asset_reads_are_owner_scoped() {
        let root = tempdir().unwrap();
        let store = MediaStudioHistoryStore::for_tests(root.path().to_path_buf());
        for index in 0..3 {
            let mut value = result();
            value.id = format!("provider-result-{index}");
            value.created_at_unix_ms += index;
            store
                .persist_image("alice", &format!("prompt-{index}"), &value)
                .await
                .unwrap();
        }

        let first = store.load_image_page("alice", None, 2).unwrap();
        assert_eq!(first.items.len(), 2);
        let cursor = first.next_cursor.clone().expect("first page cursor");
        let second = store.load_image_page("alice", Some(cursor), 2).unwrap();
        assert_eq!(second.items.len(), 1);
        assert!(second.next_cursor.is_none());
        assert!(first
            .items
            .iter()
            .all(|item| second.items.iter().all(|other| other.id != item.id)));

        let item = &first.items[0];
        let asset = &item.images[0];
        let (mime_type, bytes) = store
            .read_history_asset(&owner_hash("alice"), &item.id, &asset.content_token)
            .unwrap();
        assert_eq!(mime_type, "image/png");
        assert_eq!(bytes, b"image-bytes");
        assert!(store
            .read_history_asset(&owner_hash("bob"), &item.id, &asset.content_token)
            .is_err());

        let encoded = serde_json::to_string(&first).unwrap();
        assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
        assert!(!encoded.contains("image-bytes"));
    }

    #[test]
    fn malformed_committed_record_is_counted_without_hiding_valid_history() {
        let root = tempdir().unwrap();
        let store = MediaStudioHistoryStore::for_tests(root.path().to_path_buf());
        let account = root.path().join("Accounts").join(owner_hash("alice"));
        fs::create_dir_all(&account).unwrap();
        let record = account.join(Uuid::new_v4().to_string());
        fs::create_dir(&record).unwrap();
        fs::write(record.join("record.json"), b"{}").unwrap();

        let page = store.load_image_page("alice", None, 10).unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.unreadable_count, 1);
    }
}

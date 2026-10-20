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

    async fn asset_bytes(&self, asset: &GeneratedImageAsset) -> Result<Vec<u8>, String> {
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
            })
            .collect(),
    }
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

fn owner_hash(owner_user_id: &str) -> String {
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
}

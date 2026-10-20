use image::{guess_format, ImageFormat, ImageReader};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use super::provider::ImageGenerationInput;

const MAXIMUM_REFERENCE_IMAGES: usize = 8;
const MAXIMUM_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_SOURCE_PIXELS: u64 = 64_000_000;
const MAXIMUM_CLOSED_PRESENTATIONS: usize = 64;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaImageReferenceDraft {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub preview_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaImageReferenceImportResult {
    pub references: Vec<MediaImageReferenceDraft>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ReferenceScope {
    owner_hash: String,
    authentication_generation: u64,
    presentation_id: String,
    presentation_hash: String,
}

#[derive(Clone)]
struct ReferenceEntry {
    scope: ReferenceScope,
    id: String,
    token: String,
    name: String,
    mime_type: String,
    byte_size: u64,
    sha256: String,
    file: PathBuf,
}

impl ReferenceEntry {
    fn draft(&self) -> MediaImageReferenceDraft {
        MediaImageReferenceDraft {
            id: self.id.clone(),
            name: self.name.clone(),
            mime_type: self.mime_type.clone(),
            byte_size: self.byte_size,
            preview_url: None,
        }
    }
}

#[derive(Default)]
struct ReferenceState {
    epoch: u64,
    entries: HashMap<String, ReferenceEntry>,
    closed_presentations: HashSet<ReferenceScope>,
}

struct ReferenceVaultInner {
    root: PathBuf,
    state: Mutex<ReferenceState>,
}

#[derive(Clone)]
pub(super) struct MediaImageReferenceVault {
    inner: Arc<ReferenceVaultInner>,
}

impl MediaImageReferenceVault {
    pub(super) fn new(root: PathBuf) -> Result<Self, String> {
        if root.exists() {
            let metadata = fs::symlink_metadata(&root).map_err(|_| reference_error())?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(reference_error());
            }
            fs::remove_dir_all(&root).map_err(|_| reference_error())?;
        }
        ensure_private_directory(&root)?;
        let root = fs::canonicalize(root).map_err(|_| reference_error())?;
        Ok(Self {
            inner: Arc::new(ReferenceVaultInner {
                root,
                state: Mutex::new(ReferenceState::default()),
            }),
        })
    }

    pub(super) fn begin_scope(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(u64, String), String> {
        let scope = reference_scope(owner_user_id, authentication_generation, presentation_id)?;
        let state = self.state()?;
        if state.closed_presentations.contains(&scope) {
            return Err("图片创作页面已关闭，请重新打开后选择参考图。".into());
        }
        Ok((state.epoch, scope.presentation_id))
    }

    pub(super) fn stage_paths(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        epoch: u64,
        paths: Vec<PathBuf>,
    ) -> MediaImageReferenceImportResult {
        let scope = match reference_scope(owner_user_id, authentication_generation, presentation_id)
        {
            Ok(scope) => scope,
            Err(error) => {
                return MediaImageReferenceImportResult {
                    references: Vec::new(),
                    errors: vec![error],
                }
            }
        };
        let existing = self
            .state()
            .map(|state| {
                state
                    .entries
                    .values()
                    .filter(|entry| entry.scope == scope)
                    .count()
            })
            .unwrap_or(MAXIMUM_REFERENCE_IMAGES);
        let remaining = MAXIMUM_REFERENCE_IMAGES.saturating_sub(existing);
        let mut references = Vec::new();
        let mut errors = Vec::new();
        if paths.len() > remaining {
            errors.push(format!(
                "参考图最多只能添加 {MAXIMUM_REFERENCE_IMAGES} 张。"
            ));
        }
        for path in paths.into_iter().take(remaining) {
            match read_image_candidate(&path)
                .and_then(|candidate| self.stage_candidate(&scope, epoch, candidate))
            {
                Ok(Some(draft)) => references.push(draft),
                Ok(None) => {}
                Err(error) => errors.push(error),
            }
        }
        MediaImageReferenceImportResult { references, errors }
    }

    pub(super) fn stage_bytes(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        epoch: u64,
        values: Vec<(String, Vec<u8>)>,
    ) -> MediaImageReferenceImportResult {
        let scope = match reference_scope(owner_user_id, authentication_generation, presentation_id)
        {
            Ok(scope) => scope,
            Err(error) => {
                return MediaImageReferenceImportResult {
                    references: Vec::new(),
                    errors: vec![error],
                }
            }
        };
        let existing = self
            .state()
            .map(|state| {
                state
                    .entries
                    .values()
                    .filter(|entry| entry.scope == scope)
                    .count()
            })
            .unwrap_or(MAXIMUM_REFERENCE_IMAGES);
        let remaining = MAXIMUM_REFERENCE_IMAGES.saturating_sub(existing);
        let mut references = Vec::new();
        let mut errors = Vec::new();
        if values.len() > remaining {
            errors.push(format!(
                "参考图最多只能添加 {MAXIMUM_REFERENCE_IMAGES} 张。"
            ));
        }
        for (name, bytes) in values.into_iter().take(remaining) {
            match image_candidate_from_bytes(sanitized_name(&name), bytes)
                .and_then(|candidate| self.stage_candidate(&scope, epoch, candidate))
            {
                Ok(Some(draft)) => references.push(draft),
                Ok(None) => {}
                Err(error) => errors.push(error),
            }
        }
        MediaImageReferenceImportResult { references, errors }
    }

    pub(super) fn resolve(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        reference_ids: &[String],
    ) -> Result<Vec<ImageGenerationInput>, String> {
        if reference_ids.len() > MAXIMUM_REFERENCE_IMAGES
            || reference_ids.iter().any(|value| !valid_reference_id(value))
        {
            return Err("参考图选择无效，请重新选择。".into());
        }
        let scope = reference_scope(owner_user_id, authentication_generation, presentation_id)?;
        let entries = {
            let state = self.state()?;
            if state.closed_presentations.contains(&scope) {
                return Err("图片创作页面已关闭，请重新打开后选择参考图。".into());
            }
            let mut seen = HashSet::new();
            let mut entries = Vec::with_capacity(reference_ids.len());
            for reference_id in reference_ids {
                if !seen.insert(reference_id) {
                    return Err("参考图选择无效，请重新选择。".into());
                }
                let entry = state
                    .entries
                    .get(reference_id)
                    .filter(|entry| entry.scope == scope)
                    .cloned()
                    .ok_or_else(|| "参考图已失效，请重新选择。".to_owned())?;
                entries.push(entry);
            }
            entries
        };
        entries
            .into_iter()
            .map(|entry| {
                let bytes = read_verified_entry(&entry)?;
                Ok(ImageGenerationInput {
                    name: entry.name,
                    mime_type: entry.mime_type,
                    bytes,
                })
            })
            .collect()
    }

    pub(super) fn discard(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        reference_ids: &[String],
    ) -> Result<(), String> {
        if reference_ids.len() > MAXIMUM_REFERENCE_IMAGES
            || reference_ids.iter().any(|value| !valid_reference_id(value))
        {
            return Err("参考图选择无效，请重新选择。".into());
        }
        let scope = reference_scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut removed = Vec::new();
        {
            let mut state = self.state()?;
            for reference_id in reference_ids {
                if state
                    .entries
                    .get(reference_id)
                    .is_some_and(|entry| entry.scope == scope)
                {
                    if let Some(entry) = state.entries.remove(reference_id) {
                        removed.push(entry);
                    }
                }
            }
        }
        for entry in removed {
            let _ = fs::remove_file(entry.file);
        }
        Ok(())
    }

    pub(super) fn close_presentation(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(), String> {
        let scope = reference_scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut removed = Vec::new();
        {
            let mut state = self.state()?;
            state.epoch = state.epoch.wrapping_add(1);
            state.closed_presentations.insert(scope.clone());
            if state.closed_presentations.len() > MAXIMUM_CLOSED_PRESENTATIONS {
                state.closed_presentations.clear();
                state.closed_presentations.insert(scope.clone());
            }
            let ids = state
                .entries
                .iter()
                .filter(|(_, entry)| entry.scope == scope)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                if let Some(entry) = state.entries.remove(&id) {
                    removed.push(entry);
                }
            }
        }
        for entry in removed {
            let _ = fs::remove_file(entry.file);
        }
        let owner_directory = self.inner.root.join(&scope.owner_hash);
        let directory = owner_directory.join(&scope.presentation_hash);
        if fs::symlink_metadata(&directory)
            .ok()
            .is_some_and(|metadata| {
                metadata.file_type().is_dir() && !metadata.file_type().is_symlink()
            })
        {
            let _ = fs::remove_dir(&directory);
        }
        let _ = fs::remove_dir(owner_directory);
        Ok(())
    }

    pub(super) fn preview_asset(
        &self,
        owner_hash: &str,
        authentication_generation: u64,
        presentation_id: &str,
        token: &str,
    ) -> Result<(String, Vec<u8>), String> {
        let presentation_id = validated_presentation_id(presentation_id)?;
        if !valid_token(token) {
            return Err(reference_error());
        }
        let entry = {
            let state = self.state()?;
            state
                .entries
                .values()
                .find(|entry| {
                    entry.scope.owner_hash == owner_hash
                        && entry.scope.authentication_generation == authentication_generation
                        && entry.scope.presentation_id == presentation_id
                        && entry.token == token
                })
                .cloned()
                .ok_or_else(reference_error)?
        };
        let bytes = read_verified_entry(&entry)?;
        Ok((entry.mime_type, bytes))
    }

    pub(super) fn token_for_reference(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        reference_id: &str,
    ) -> Option<String> {
        let scope =
            reference_scope(owner_user_id, authentication_generation, presentation_id).ok()?;
        self.state()
            .ok()?
            .entries
            .get(reference_id)
            .filter(|entry| entry.scope == scope)
            .map(|entry| entry.token.clone())
    }

    pub(super) fn reset(&self) {
        let entries = if let Ok(mut state) = self.state() {
            state.epoch = state.epoch.wrapping_add(1);
            state.closed_presentations.clear();
            state
                .entries
                .drain()
                .map(|(_, entry)| entry)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for entry in entries {
            let _ = fs::remove_file(entry.file);
        }
    }

    fn stage_candidate(
        &self,
        scope: &ReferenceScope,
        epoch: u64,
        candidate: ImageCandidate,
    ) -> Result<Option<MediaImageReferenceDraft>, String> {
        {
            let state = self.state()?;
            if state.epoch != epoch || state.closed_presentations.contains(scope) {
                return Err("图片创作页面已关闭，请重新打开后选择参考图。".into());
            }
            if state
                .entries
                .values()
                .any(|entry| entry.scope == *scope && entry.sha256 == candidate.sha256)
            {
                return Ok(None);
            }
            if state
                .entries
                .values()
                .filter(|entry| entry.scope == *scope)
                .count()
                >= MAXIMUM_REFERENCE_IMAGES
            {
                return Err(format!(
                    "参考图最多只能添加 {MAXIMUM_REFERENCE_IMAGES} 张。"
                ));
            }
        }
        let directory = ensure_scoped_directory(
            &self.inner.root,
            &scope.owner_hash,
            &scope.presentation_hash,
        )?;
        let token = Uuid::new_v4().to_string();
        let file = directory.join(&token);
        write_private_file(&file, &candidate.bytes)?;
        let entry = ReferenceEntry {
            scope: scope.clone(),
            id: format!("reference_{}", Uuid::new_v4().simple()),
            token,
            name: candidate.name,
            mime_type: candidate.mime_type,
            byte_size: candidate.bytes.len() as u64,
            sha256: candidate.sha256,
            file: file.clone(),
        };
        let draft = entry.draft();
        let mut state = self.state()?;
        if state.epoch != epoch || state.closed_presentations.contains(scope) {
            drop(state);
            let _ = fs::remove_file(file);
            return Err("图片创作页面已关闭，请重新打开后选择参考图。".into());
        }
        if state
            .entries
            .values()
            .filter(|existing| existing.scope == *scope)
            .count()
            >= MAXIMUM_REFERENCE_IMAGES
        {
            drop(state);
            let _ = fs::remove_file(file);
            return Err(format!(
                "参考图最多只能添加 {MAXIMUM_REFERENCE_IMAGES} 张。"
            ));
        }
        if state
            .entries
            .values()
            .any(|existing| existing.scope == *scope && existing.sha256 == entry.sha256)
        {
            drop(state);
            let _ = fs::remove_file(file);
            return Ok(None);
        }
        state.entries.insert(entry.id.clone(), entry);
        Ok(Some(draft))
    }

    fn state(&self) -> Result<std::sync::MutexGuard<'_, ReferenceState>, String> {
        self.inner.state.lock().map_err(|_| reference_error())
    }
}

struct ImageCandidate {
    name: String,
    mime_type: String,
    bytes: Vec<u8>,
    sha256: String,
}

fn read_image_candidate(path: &Path) -> Result<ImageCandidate, String> {
    let name = sanitized_name(
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("参考图"),
    );
    let link_metadata = fs::symlink_metadata(path).map_err(|_| "无法读取参考图。".to_owned())?;
    if !link_metadata.file_type().is_file()
        || link_metadata.file_type().is_symlink()
        || link_metadata.len() == 0
        || link_metadata.len() > MAXIMUM_IMAGE_BYTES as u64
    {
        return Err(format!("“{name}”不是有效图片或超过 20 MB。"));
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
    let mut file = options
        .open(path)
        .map_err(|_| "无法读取参考图。".to_owned())?;
    let before = file.metadata().map_err(|_| "无法读取参考图。".to_owned())?;
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take((MAXIMUM_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取参考图。".to_owned())?;
    let after = file.metadata().map_err(|_| "无法读取参考图。".to_owned())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
        || bytes.is_empty()
        || bytes.len() > MAXIMUM_IMAGE_BYTES
    {
        return Err(format!("“{name}”在读取期间发生变化或超过 20 MB。"));
    }
    image_candidate_from_bytes(name, bytes)
}

fn image_candidate_from_bytes(name: String, bytes: Vec<u8>) -> Result<ImageCandidate, String> {
    if bytes.is_empty() || bytes.len() > MAXIMUM_IMAGE_BYTES {
        return Err(format!("“{name}”不是有效图片或超过 20 MB。"));
    }
    let format = guess_format(&bytes).map_err(|_| format!("“{name}”不是有效图片。"))?;
    let mime_type = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::WebP => "image/webp",
        _ => return Err(format!("“{name}”仅支持 PNG、JPEG 或 WebP。")),
    }
    .to_owned();
    let (width, height) = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| format!("“{name}”无法解码。"))?;
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0 || height == 0 || pixels > MAXIMUM_SOURCE_PIXELS {
        return Err(format!("“{name}”像素尺寸无效或过大。"));
    }
    let sha256 = digest(&bytes);
    Ok(ImageCandidate {
        name,
        mime_type,
        bytes,
        sha256,
    })
}

fn read_verified_entry(entry: &ReferenceEntry) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(&entry.file).map_err(|_| reference_error())?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != entry.byte_size
        || metadata.len() == 0
        || metadata.len() > MAXIMUM_IMAGE_BYTES as u64
    {
        return Err(reference_error());
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
    let mut file = options.open(&entry.file).map_err(|_| reference_error())?;
    let before = file.metadata().map_err(|_| reference_error())?;
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take((MAXIMUM_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| reference_error())?;
    let after = file.metadata().map_err(|_| reference_error())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != entry.byte_size as usize
        || digest(&bytes) != entry.sha256
    {
        return Err(reference_error());
    }
    Ok(bytes)
}

fn reference_scope(
    owner_user_id: &str,
    authentication_generation: u64,
    presentation_id: &str,
) -> Result<ReferenceScope, String> {
    if owner_user_id.trim().is_empty() {
        return Err(reference_error());
    }
    let presentation_id = validated_presentation_id(presentation_id)?;
    Ok(ReferenceScope {
        owner_hash: digest(owner_user_id.as_bytes()),
        authentication_generation,
        presentation_hash: digest(presentation_id.as_bytes()),
        presentation_id,
    })
}

fn validated_presentation_id(value: &str) -> Result<String, String> {
    let uuid = Uuid::parse_str(value).map_err(|_| "图片创作页面标识无效。".to_owned())?;
    if uuid.to_string() != value {
        return Err("图片创作页面标识无效。".into());
    }
    Ok(uuid.to_string())
}

fn valid_token(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.to_string() == value)
        .unwrap_or(false)
}

fn valid_reference_id(value: &str) -> bool {
    value.strip_prefix("reference_").is_some_and(|token| {
        token.len() == 32
            && token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn ensure_scoped_directory(
    root: &Path,
    owner_hash: &str,
    presentation_hash: &str,
) -> Result<PathBuf, String> {
    let owner = root.join(owner_hash);
    ensure_private_directory(&owner)?;
    validate_directory_beneath(root, &owner)?;
    let presentation = owner.join(presentation_hash);
    ensure_private_directory(&presentation)?;
    validate_directory_beneath(root, &presentation)?;
    Ok(presentation)
}

fn validate_directory_beneath(root: &Path, directory: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(directory).map_err(|_| reference_error())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(reference_error());
    }
    let canonical = fs::canonicalize(directory).map_err(|_| reference_error())?;
    if canonical != directory || !canonical.starts_with(root) {
        return Err(reference_error());
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|_| reference_error())?;
    let metadata = fs::symlink_metadata(path).map_err(|_| reference_error())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(reference_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| reference_error())?;
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
    let mut file = options.open(path).map_err(|_| reference_error())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| reference_error())
}

fn sanitized_name(value: &str) -> String {
    let value = value
        .chars()
        .filter(|character| !character.is_control())
        .take(255)
        .collect::<String>();
    if value.trim().is_empty() {
        "参考图".into()
    } else {
        value
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn reference_error() -> String {
    "参考图无效或已失效，请重新选择。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    use tempfile::tempdir;

    fn png_bytes() -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([1, 2, 3, 4])));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn vault_keeps_paths_and_base64_native_and_binds_owner_generation_presentation() {
        let root = tempdir().unwrap();
        let vault = MediaImageReferenceVault::new(root.path().join("refs")).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let file = root.path().join("source.png");
        fs::write(&file, png_bytes()).unwrap();
        let (epoch, _) = vault.begin_scope("alice", 7, &presentation).unwrap();
        let imported = vault.stage_paths("alice", 7, &presentation, epoch, vec![file]);
        assert!(imported.errors.is_empty());
        assert_eq!(imported.references.len(), 1);
        let draft = &imported.references[0];
        let encoded = serde_json::to_string(draft).unwrap();
        assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
        assert!(!encoded.contains("base64"));

        let resolved = vault
            .resolve("alice", 7, &presentation, std::slice::from_ref(&draft.id))
            .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].mime_type, "image/png");
        assert_eq!(resolved[0].bytes, png_bytes());
        let token = vault
            .token_for_reference("alice", 7, &presentation, &draft.id)
            .unwrap();
        let (mime_type, preview) = vault
            .preview_asset(&digest(b"alice"), 7, &presentation, &token)
            .unwrap();
        assert_eq!(mime_type, "image/png");
        assert_eq!(preview, png_bytes());
        assert!(vault
            .resolve("bob", 7, &presentation, std::slice::from_ref(&draft.id))
            .is_err());
        assert!(vault
            .resolve("alice", 8, &presentation, std::slice::from_ref(&draft.id))
            .is_err());
    }

    #[test]
    fn generated_history_bytes_use_the_same_validation_and_deduplication_boundary() {
        let root = tempdir().unwrap();
        let vault = MediaImageReferenceVault::new(root.path().join("refs")).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let (epoch, _) = vault.begin_scope("alice", 7, &presentation).unwrap();
        let bytes = png_bytes();
        let first = vault.stage_bytes(
            "alice",
            7,
            &presentation,
            epoch,
            vec![("generated.png".into(), bytes.clone())],
        );
        assert!(first.errors.is_empty());
        assert_eq!(first.references.len(), 1);
        let duplicate = vault.stage_bytes(
            "alice",
            7,
            &presentation,
            epoch,
            vec![("same.png".into(), bytes)],
        );
        assert!(duplicate.errors.is_empty());
        assert!(duplicate.references.is_empty());
    }

    #[test]
    fn close_and_reset_reject_late_imports_and_remove_active_entries() {
        let root = tempdir().unwrap();
        let vault = MediaImageReferenceVault::new(root.path().join("refs")).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let file = root.path().join("source.png");
        fs::write(&file, png_bytes()).unwrap();
        let (epoch, _) = vault.begin_scope("alice", 7, &presentation).unwrap();
        vault.close_presentation("alice", 7, &presentation).unwrap();
        let late = vault.stage_paths("alice", 7, &presentation, epoch, vec![file.clone()]);
        assert!(late.references.is_empty());
        assert!(!late.errors.is_empty());

        let next_presentation = Uuid::new_v4().to_string();
        let (next_epoch, _) = vault.begin_scope("alice", 7, &next_presentation).unwrap();
        let imported = vault.stage_paths("alice", 7, &next_presentation, next_epoch, vec![file]);
        let id = imported.references[0].id.clone();
        vault.reset();
        assert!(vault
            .resolve("alice", 7, &next_presentation, &[id])
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn owner_scope_symlink_is_rejected_without_writing_outside_the_vault() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let vault_root = root.path().join("refs");
        let vault = MediaImageReferenceVault::new(vault_root.clone()).unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, vault_root.join(digest(b"alice"))).unwrap();
        let source = root.path().join("source.png");
        fs::write(&source, png_bytes()).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let (epoch, _) = vault.begin_scope("alice", 7, &presentation).unwrap();
        let imported = vault.stage_paths("alice", 7, &presentation, epoch, vec![source]);
        assert!(imported.references.is_empty());
        assert!(!imported.errors.is_empty());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
}

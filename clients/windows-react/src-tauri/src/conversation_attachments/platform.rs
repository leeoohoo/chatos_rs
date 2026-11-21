use super::*;

pub(super) struct AttachmentCandidate {
    pub(super) name: String,
    pub(super) media_type: String,
    pub(super) kind: AttachmentKind,
    pub(super) origin: AttachmentOrigin,
    pub(super) bytes: Vec<u8>,
}

pub(super) enum ClipboardPayload {
    Image(AttachmentCandidate),
    LongText(AttachmentCandidate),
    Text(String),
    Empty,
}

pub(super) fn read_file_candidate(path: &Path) -> Result<AttachmentCandidate, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitized_name)
        .unwrap_or_else(|| "附件".into());
    let link_metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法读取“{name}”：{error}"))?;
    if !link_metadata.file_type().is_file() || link_metadata.file_type().is_symlink() {
        return Err(format!("“{name}”不是可发送的文件"));
    }
    if link_metadata.len() > MAXIMUM_ATTACHMENT_BYTES as u64 {
        return Err(format!("“{name}”超过 20 MB"));
    }
    let mut options = fs::OpenOptions::new();
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
    let mut handle = options
        .open(path)
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    let before = handle
        .metadata()
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    if !before.file_type().is_file()
        || before.len() == 0
        || before.len() != link_metadata.len()
        || before.len() > MAXIMUM_ATTACHMENT_BYTES as u64
    {
        return Err(format!("“{name}”不是可发送的文件"));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut handle)
        .take((MAXIMUM_ATTACHMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    let after = handle
        .metadata()
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    if bytes.len() > MAXIMUM_ATTACHMENT_BYTES {
        return Err(format!("“{name}”超过 20 MB"));
    }
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
    {
        return Err(format!("“{name}”在读取期间发生了变化，请重新添加"));
    }
    let media_type = mime_guess::from_path(&name)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_owned();
    Ok(AttachmentCandidate {
        name,
        kind: attachment_kind(&media_type),
        media_type,
        origin: AttachmentOrigin::File,
        bytes,
    })
}

pub(super) fn read_clipboard_payload() -> Result<ClipboardPayload, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| format!("无法访问剪贴板：{error}"))?;
    if let Ok(image) = clipboard.get_image() {
        let expected = image
            .width
            .checked_mul(image.height)
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| "剪贴板图片尺寸无效。".to_string())?;
        if expected != image.bytes.len() || expected > 160 * 1_024 * 1_024 {
            return Err("剪贴板图片过大或格式无效。".into());
        }
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(
                Cursor::new(&mut encoded),
                image.width as u32,
                image.height as u32,
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|error| format!("无法处理剪贴板图片：{error}"))?;
            writer
                .write_image_data(image.bytes.as_ref())
                .map_err(|error| format!("无法处理剪贴板图片：{error}"))?;
        }
        return Ok(ClipboardPayload::Image(AttachmentCandidate {
            name: pasted_name("粘贴的图片", "png"),
            media_type: "image/png".into(),
            kind: AttachmentKind::Image,
            origin: AttachmentOrigin::PastedImage,
            bytes: encoded,
        }));
    }
    match clipboard.get_text() {
        Ok(text) if !text.is_empty() => {
            if text.chars().count() >= LONG_PASTE_CHARACTER_THRESHOLD
                || text.len() >= LONG_PASTE_BYTE_THRESHOLD
            {
                Ok(ClipboardPayload::LongText(AttachmentCandidate {
                    name: pasted_name("粘贴的长文本", "txt"),
                    media_type: "text/plain".into(),
                    kind: AttachmentKind::File,
                    origin: AttachmentOrigin::PastedText,
                    bytes: text.into_bytes(),
                }))
            } else {
                Ok(ClipboardPayload::Text(text))
            }
        }
        _ => Ok(ClipboardPayload::Empty),
    }
}

pub(super) fn ensure_private_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir(path).map_err(|error| format!("无法创建附件保险库：{error}"))?;
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法验证附件保险库：{error}"))?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
    {
        return Err("附件保险库目录无效。".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("无法保护附件保险库：{error}"))?;
    }
    Ok(())
}

pub(super) fn write_metadata(directory: &Path, metadata: &VaultMetadata) -> Result<(), String> {
    let bytes =
        serde_json::to_vec(metadata).map_err(|error| format!("无法记录附件元数据：{error}"))?;
    let target = metadata_path(directory, &metadata.token);
    replace_owned_file(directory, &target, ".attachment-metadata-", &bytes)
        .map_err(|error| format!("无法记录附件元数据：{error}"))
}

pub(super) fn read_metadata(directory: &Path, token: &str) -> Result<VaultMetadata, String> {
    let path = metadata_path(directory, token);
    let metadata =
        fs::symlink_metadata(&path).map_err(|error| format!("无法读取附件元数据：{error}"))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
    {
        return Err("附件元数据不是普通文件。".into());
    }
    let mut bytes = Vec::with_capacity(
        metadata.len().min(MAXIMUM_ATTACHMENT_METADATA_BYTES as u64) as usize + 1,
    );
    fs::File::open(&path)
        .map_err(|error| format!("无法读取附件元数据：{error}"))?
        .take((MAXIMUM_ATTACHMENT_METADATA_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取附件元数据：{error}"))?;
    if bytes.len() > MAXIMUM_ATTACHMENT_METADATA_BYTES {
        return Err("附件元数据超过大小限制。".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("附件元数据无效：{error}"))
}

pub(super) fn metadata_path(directory: &Path, token: &str) -> PathBuf {
    directory.join(format!("{token}.json"))
}

pub(super) fn attachment_kind(media_type: &str) -> AttachmentKind {
    if media_type.starts_with("image/") {
        AttachmentKind::Image
    } else if media_type.starts_with("audio/") {
        AttachmentKind::Audio
    } else {
        AttachmentKind::File
    }
}

pub(super) fn safe_preview_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

pub(super) fn is_text_like(media_type: &str) -> bool {
    media_type.starts_with("text/")
        || matches!(
            media_type,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/x-yaml"
        )
}

pub(super) fn preview_url(scope: &ScopeKey, token: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!(
            "http://chatos-attachment.localhost/{}/{}/{}",
            scope.owner_hash, scope.conversation_hash, token
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!(
            "chatos-attachment://localhost/{}/{}/{}",
            scope.owner_hash, scope.conversation_hash, token
        )
    }
}

pub(super) fn sanitized_name(value: impl AsRef<str>) -> String {
    let value = value
        .as_ref()
        .chars()
        .filter(|character| !character.is_control())
        .take(255)
        .collect::<String>();
    if value.trim().is_empty() {
        "附件".into()
    } else {
        value
    }
}

pub(super) fn pasted_name(prefix: &str, extension: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("{prefix} {timestamp}.{extension}")
}

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}

pub(super) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn valid_token(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.to_string().to_lowercase() == value)
        .unwrap_or(false)
}

pub(super) fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

impl AttachmentManager {
    pub(super) fn protocol_response(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> http::Response<Vec<u8>> {
        match self.protocol_body(request.uri().path()) {
            Ok((media_type, body)) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, media_type)
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(body)
                .expect("valid attachment preview response"),
            Err(status) => http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(b"Attachment preview unavailable".to_vec())
                .expect("valid attachment error response"),
        }
    }

    fn protocol_body(&self, path: &str) -> Result<(String, Vec<u8>), http::StatusCode> {
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() != 3
            || !valid_hash(parts[0])
            || !valid_hash(parts[1])
            || !valid_token(parts[2])
        {
            return Err(http::StatusCode::BAD_REQUEST);
        }
        let scope = ScopeKey {
            owner_hash: parts[0].to_owned(),
            conversation_hash: parts[1].to_owned(),
        };
        let directory = self
            .validated_scope_directory(&scope)
            .map_err(|_| http::StatusCode::NOT_FOUND)?;
        let metadata =
            read_metadata(&directory, parts[2]).map_err(|_| http::StatusCode::NOT_FOUND)?;
        if metadata.token != parts[2] || !safe_preview_media_type(&metadata.media_type) {
            return Err(http::StatusCode::FORBIDDEN);
        }
        let body = self
            .read_verified_file(&scope, &metadata)
            .map_err(|_| http::StatusCode::NOT_FOUND)?;
        Ok((metadata.media_type, body))
    }

    pub(super) fn verify_file(
        &self,
        scope: &ScopeKey,
        metadata: &VaultMetadata,
    ) -> Result<(), String> {
        self.read_verified_file(scope, metadata).map(|_| ())
    }

    pub(super) fn read_verified_file(
        &self,
        scope: &ScopeKey,
        metadata: &VaultMetadata,
    ) -> Result<Vec<u8>, String> {
        let directory = self.validated_scope_directory(scope)?;
        let file = directory.join(&metadata.token);
        let link_metadata =
            fs::symlink_metadata(&file).map_err(|_| "附件已失效，请重新添加。".to_string())?;
        if !link_metadata.file_type().is_file() || link_metadata.file_type().is_symlink() {
            return Err("附件完整性校验失败，请重新添加。".into());
        }
        let mut options = fs::OpenOptions::new();
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
        let mut handle = options
            .open(file)
            .map_err(|_| "附件已失效，请重新添加。".to_string())?;
        let before = handle
            .metadata()
            .map_err(|_| "附件完整性校验失败，请重新添加。".to_string())?;
        if !before.file_type().is_file()
            || before.len() != metadata.byte_size
            || metadata.byte_size == 0
            || metadata.byte_size > MAXIMUM_ATTACHMENT_BYTES as u64
        {
            return Err("附件完整性校验失败，请重新添加。".into());
        }
        let mut bytes = Vec::with_capacity(metadata.byte_size as usize);
        (&mut handle)
            .take((MAXIMUM_ATTACHMENT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "附件完整性校验失败，请重新添加。".to_string())?;
        let after = handle
            .metadata()
            .map_err(|_| "附件完整性校验失败，请重新添加。".to_string())?;
        if before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || bytes.len() != metadata.byte_size as usize
            || digest(&bytes) != metadata.sha256
        {
            return Err("附件完整性校验失败，请重新添加。".into());
        }
        Ok(bytes)
    }

    pub(super) fn record_metadata(
        &self,
        scope: &ScopeKey,
        record: &LocalConversationAttachmentRecord,
    ) -> Result<VaultMetadata, String> {
        let token = record
            .authorized_local_ref
            .strip_prefix(REFERENCE_PREFIX)
            .filter(|token| valid_token(token))
            .ok_or_else(|| "The attachment reference is invalid.".to_string())?;
        let directory = self.validated_scope_directory(scope)?;
        let metadata = read_metadata(&directory, token)
            .map_err(|_| "The authorized attachment is unavailable.".to_string())?;
        if metadata.token != token
            || metadata.attachment_id != record.attachment_id
            || metadata.display_name != record.display_name
            || metadata.media_type != record.media_type
            || metadata.byte_size != record.byte_size
            || metadata.sha256 != record.sha256
        {
            return Err("The authorized attachment failed its integrity check.".into());
        }
        Ok(metadata)
    }

    #[cfg(test)]
    pub(super) fn scope_directory(&self, scope: &ScopeKey) -> PathBuf {
        self.inner
            .root
            .join(&scope.owner_hash)
            .join(&scope.conversation_hash)
    }

    pub(super) fn ensure_scope_directory(&self, scope: &ScopeKey) -> Result<PathBuf, String> {
        let owner = self.inner.root.join(&scope.owner_hash);
        ensure_private_directory(&owner)?;
        let directory = owner.join(&scope.conversation_hash);
        ensure_private_directory(&directory)?;
        self.validated_scope_directory(scope)
    }

    pub(super) fn validated_scope_directory(&self, scope: &ScopeKey) -> Result<PathBuf, String> {
        let owner = self.inner.root.join(&scope.owner_hash);
        let directory = owner.join(&scope.conversation_hash);
        for candidate in [&owner, &directory] {
            let metadata = fs::symlink_metadata(candidate)
                .map_err(|_| "附件保险库作用域不可用。".to_string())?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err("附件保险库作用域无效。".into());
            }
        }
        let canonical =
            fs::canonicalize(&directory).map_err(|_| "附件保险库作用域不可用。".to_string())?;
        if !canonical.starts_with(&self.inner.root) || canonical != directory {
            return Err("附件保险库作用域无效。".into());
        }
        Ok(directory)
    }

    pub(super) fn state(&self) -> Result<std::sync::MutexGuard<'_, AttachmentState>, String> {
        self.inner
            .state
            .lock()
            .map_err(|_| "附件状态不可用。".to_string())
    }
}

#[cfg(windows)]
pub(super) fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(super) fn is_windows_reparse_point(_: &fs::Metadata) -> bool {
    false
}

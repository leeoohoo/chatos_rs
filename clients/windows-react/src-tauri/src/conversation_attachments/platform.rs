struct AttachmentCandidate {
    name: String,
    media_type: String,
    kind: AttachmentKind,
    origin: AttachmentOrigin,
    bytes: Vec<u8>,
}

enum ClipboardPayload {
    Image(AttachmentCandidate),
    LongText(AttachmentCandidate),
    Text(String),
    Empty,
}

fn read_file_candidate(path: &Path) -> Result<AttachmentCandidate, String> {
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

fn read_clipboard_payload() -> Result<ClipboardPayload, String> {
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

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir(path).map_err(|error| format!("无法创建附件保险库：{error}"))?;
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法验证附件保险库：{error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
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

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("无法保存附件：{error}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("无法保存附件：{error}"))
}

fn write_metadata(directory: &Path, metadata: &VaultMetadata) -> Result<(), String> {
    let bytes =
        serde_json::to_vec(metadata).map_err(|error| format!("无法记录附件元数据：{error}"))?;
    let target = metadata_path(directory, &metadata.token);
    let temporary = directory.join(format!("{}.json.tmp", metadata.token));
    write_private_file(&temporary, &bytes)?;
    fs::rename(&temporary, target).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("无法记录附件元数据：{error}")
    })
}

fn read_metadata(directory: &Path, token: &str) -> Result<VaultMetadata, String> {
    let bytes = fs::read(metadata_path(directory, token))
        .map_err(|error| format!("无法读取附件元数据：{error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("附件元数据无效：{error}"))
}

fn metadata_path(directory: &Path, token: &str) -> PathBuf {
    directory.join(format!("{token}.json"))
}

fn attachment_kind(media_type: &str) -> AttachmentKind {
    if media_type.starts_with("image/") {
        AttachmentKind::Image
    } else if media_type.starts_with("audio/") {
        AttachmentKind::Audio
    } else {
        AttachmentKind::File
    }
}

fn safe_preview_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

fn is_text_like(media_type: &str) -> bool {
    media_type.starts_with("text/")
        || matches!(
            media_type,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/x-yaml"
        )
}

fn preview_url(scope: &ScopeKey, token: &str) -> String {
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

fn sanitized_name(value: impl AsRef<str>) -> String {
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

fn pasted_name(prefix: &str, extension: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("{prefix} {timestamp}.{extension}")
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_token(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.to_string().to_lowercase() == value)
        .unwrap_or(false)
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

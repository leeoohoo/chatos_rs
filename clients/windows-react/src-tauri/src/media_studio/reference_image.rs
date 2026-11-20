use super::{digest, reference_error, sanitized_name, ReferenceEntry};
use image::{guess_format, ImageFormat, ImageReader};
use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Read},
    path::Path,
};

const MAXIMUM_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_SOURCE_PIXELS: u64 = 64_000_000;

pub(super) struct ImageCandidate {
    pub(super) name: String,
    pub(super) mime_type: String,
    pub(super) bytes: Vec<u8>,
    pub(super) sha256: String,
}

pub(super) fn read_image_candidate(path: &Path) -> Result<ImageCandidate, String> {
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

pub(super) fn image_candidate_from_bytes(
    name: String,
    bytes: Vec<u8>,
) -> Result<ImageCandidate, String> {
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

pub(super) fn read_verified_entry(entry: &ReferenceEntry) -> Result<Vec<u8>, String> {
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

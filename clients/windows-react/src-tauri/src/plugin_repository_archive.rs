use super::MAXIMUM_ARCHIVE_BYTES;
use crate::app_data_transaction::ensure_owned_directory;
use flate2::read::GzDecoder;
use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    io::{Cursor, Read, Write},
    path::Path,
};
use tar::Archive;
use zip::ZipArchive;

const MAXIMUM_ARCHIVE_ENTRIES: usize = 20_000;
const MAXIMUM_ARCHIVE_UNPACKED_BYTES: u64 = 512 * 1_024 * 1_024;

pub(crate) fn extract_release_archive(
    bytes: &[u8],
    asset_name: &str,
    destination: &Path,
) -> Result<(), String> {
    ensure_owned_directory(destination).map_err(|_| "发行包目标目录无效。".to_owned())?;
    let entries = archive_entries(bytes, asset_name)?;
    let mut seen = BTreeSet::new();
    for entry in &entries {
        if !seen.insert(entry.trim_end_matches('/').to_lowercase()) {
            return Err("发行包包含重复或大小写冲突的路径。".into());
        }
    }
    if asset_name.ends_with(".zip") {
        let mut archive =
            ZipArchive::new(Cursor::new(bytes)).map_err(|_| "ZIP 发行包无法读取。".to_owned())?;
        for index in 0..archive.len() {
            let mut item = archive
                .by_index(index)
                .map_err(|_| "ZIP 发行包条目无效。".to_owned())?;
            let name = item
                .name()
                .map_err(|_| "ZIP 发行包路径无效。".to_owned())?
                .into_owned();
            validate_archive_path(&name)?;
            let target = destination.join(name.trim_end_matches('/'));
            if item.is_dir() {
                ensure_owned_directory(&target).map_err(|_| "无法创建发行包目录。".to_owned())?;
                continue;
            }
            let parent = target
                .parent()
                .ok_or_else(|| "发行包目标路径无效。".to_owned())?;
            ensure_owned_directory(parent).map_err(|_| "无法创建发行包目录。".to_owned())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|_| "无法写入发行包文件。".to_owned())?;
            let mut copied = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = item
                    .read(&mut buffer)
                    .map_err(|_| "发行包文件读取失败。".to_owned())?;
                if read == 0 {
                    break;
                }
                copied = copied.saturating_add(read as u64);
                if copied > MAXIMUM_ARCHIVE_UNPACKED_BYTES {
                    return Err("发行包解压体积超限。".into());
                }
                output
                    .write_all(&buffer[..read])
                    .map_err(|_| "发行包文件写入失败。".to_owned())?;
            }
            output
                .sync_all()
                .map_err(|_| "发行包文件提交失败。".to_owned())?;
        }
    } else {
        let decoder = GzDecoder::new(Cursor::new(bytes));
        let mut archive = Archive::new(decoder);
        for item in archive
            .entries()
            .map_err(|_| "发行包归档无法读取。".to_owned())?
        {
            let mut item = item.map_err(|_| "发行包归档条目无效。".to_owned())?;
            let name = item
                .path()
                .map_err(|_| "发行包包含无效路径。".to_owned())?
                .to_str()
                .ok_or_else(|| "发行包包含非 UTF-8 路径。".to_owned())?
                .to_owned();
            validate_archive_path(&name)?;
            let target = destination.join(name.trim_end_matches('/'));
            if item.header().entry_type().is_dir() {
                ensure_owned_directory(&target).map_err(|_| "无法创建发行包目录。".to_owned())?;
                continue;
            }
            let parent = target
                .parent()
                .ok_or_else(|| "发行包目标路径无效。".to_owned())?;
            ensure_owned_directory(parent).map_err(|_| "无法创建发行包目录。".to_owned())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|_| "无法写入发行包文件。".to_owned())?;
            std::io::copy(&mut item, &mut output).map_err(|_| "发行包文件写入失败。".to_owned())?;
            output
                .sync_all()
                .map_err(|_| "发行包文件提交失败。".to_owned())?;
        }
    }
    Ok(())
}

pub(super) fn archive_entries(bytes: &[u8], asset_name: &str) -> Result<Vec<String>, String> {
    if asset_name.ends_with(".zip") {
        zip_entries(bytes)
    } else {
        tar_gz_entries(bytes)
    }
}

fn tar_gz_entries(bytes: &[u8]) -> Result<Vec<String>, String> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(decoder);
    let mut entries = Vec::new();
    let mut total = 0u64;
    for item in archive
        .entries()
        .map_err(|_| "发行包归档无法读取。".to_owned())?
    {
        let entry = item.map_err(|_| "发行包归档条目无效。".to_owned())?;
        if entries.len() >= MAXIMUM_ARCHIVE_ENTRIES {
            return Err("发行包文件数量异常或超限。".into());
        }
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err("发行包包含链接或特殊文件。".into());
        }
        let path = entry
            .path()
            .map_err(|_| "发行包包含无效路径。".to_owned())?
            .to_str()
            .ok_or_else(|| "发行包包含非 UTF-8 路径。".to_owned())?
            .to_owned();
        validate_archive_path(&path)?;
        total = total.saturating_add(entry.header().size().unwrap_or(u64::MAX));
        if total > MAXIMUM_ARCHIVE_UNPACKED_BYTES {
            return Err("发行包解压体积超限。".into());
        }
        entries.push(path);
    }
    if entries.is_empty() {
        return Err("发行包没有可用文件。".into());
    }
    Ok(entries)
}

fn zip_entries(bytes: &[u8]) -> Result<Vec<String>, String> {
    let start = bytes.len().saturating_sub(65_557);
    let eocd = bytes[start..]
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .map(|offset| start + offset)
        .ok_or_else(|| "ZIP 发行包缺少中央目录。".to_owned())?;
    let count = read_u16(bytes, eocd + 10)? as usize;
    let directory_size = read_u32(bytes, eocd + 12)? as usize;
    let directory_offset = read_u32(bytes, eocd + 16)? as usize;
    if count == 0
        || count > MAXIMUM_ARCHIVE_ENTRIES
        || directory_offset
            .checked_add(directory_size)
            .is_none_or(|end| end > bytes.len())
    {
        return Err("ZIP 发行包中央目录无效或超限。".into());
    }
    let mut cursor = directory_offset;
    let mut entries = Vec::with_capacity(count);
    let mut total_unpacked = 0u64;
    for _ in 0..count {
        if cursor + 46 > bytes.len() || &bytes[cursor..cursor + 4] != b"PK\x01\x02" {
            return Err("ZIP 发行包条目无效。".into());
        }
        let flags = read_u16(bytes, cursor + 8)?;
        let compression = read_u16(bytes, cursor + 10)?;
        let compressed_size = read_u32(bytes, cursor + 20)? as u64;
        let uncompressed_size = read_u32(bytes, cursor + 24)? as u64;
        let name_len = read_u16(bytes, cursor + 28)? as usize;
        let extra_len = read_u16(bytes, cursor + 30)? as usize;
        let comment_len = read_u16(bytes, cursor + 32)? as usize;
        let external_attributes = read_u32(bytes, cursor + 38)?;
        let end = cursor
            .checked_add(46)
            .and_then(|value| value.checked_add(name_len))
            .and_then(|value| value.checked_add(extra_len))
            .and_then(|value| value.checked_add(comment_len))
            .ok_or_else(|| "ZIP 发行包条目超限。".to_owned())?;
        if end > bytes.len() || compressed_size > MAXIMUM_ARCHIVE_BYTES as u64 {
            return Err("ZIP 发行包条目超限。".into());
        }
        let name = std::str::from_utf8(&bytes[cursor + 46..cursor + 46 + name_len])
            .map_err(|_| "ZIP 发行包包含非 UTF-8 路径。".to_owned())?
            .to_owned();
        validate_archive_path(&name)?;
        let is_directory = name.ends_with('/');
        let unix_mode = external_attributes >> 16;
        if (unix_mode & 0xf000) == 0xa000 || (flags & 0x1) != 0 {
            return Err("ZIP 发行包包含链接、加密或特殊文件。".into());
        }
        if !is_directory && compression != 0 && compression != 8 {
            return Err("ZIP 发行包使用了不支持的压缩方式。".into());
        }
        if uncompressed_size > MAXIMUM_ARCHIVE_UNPACKED_BYTES {
            return Err("ZIP 发行包解压体积超限。".into());
        }
        total_unpacked = total_unpacked.saturating_add(uncompressed_size);
        if total_unpacked > MAXIMUM_ARCHIVE_UNPACKED_BYTES {
            return Err("ZIP 发行包解压体积超限。".into());
        }
        entries.push(name);
        cursor = end;
    }
    Ok(entries)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "归档目录越界。".to_owned())?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "归档目录越界。".to_owned())?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

pub(super) fn validate_archive_path(value: &str) -> Result<(), String> {
    let value = value.trim_end_matches('/');
    if value.is_empty()
        || value.contains('\\')
        || value.contains('\0')
        || value.starts_with('/')
        || (value.as_bytes().get(1) == Some(&b':') && value.as_bytes()[0].is_ascii_alphabetic())
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("发行包包含不安全路径。".into());
    }
    if Path::new(value).components().any(|component| {
        matches!(
            component,
            std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::ParentDir
        )
    }) {
        return Err("发行包包含不安全路径。".into());
    }
    Ok(())
}

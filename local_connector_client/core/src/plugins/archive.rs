// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::EntryType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginPackageLimits {
    pub max_package_bytes: u64,
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_unpacked_bytes: u64,
    pub max_path_bytes: usize,
    pub max_path_depth: usize,
}

impl Default for PluginPackageLimits {
    fn default() -> Self {
        Self {
            max_package_bytes: 256 * 1024 * 1024,
            max_entries: 8_192,
            max_file_bytes: 128 * 1024 * 1024,
            max_unpacked_bytes: 768 * 1024 * 1024,
            max_path_bytes: 512,
            max_path_depth: 48,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPackageFiles {
    pub root: PathBuf,
    pub file_sha256: BTreeMap<String, String>,
    pub unpacked_bytes: u64,
}

pub fn sha256_file(path: &Path, max_bytes: u64) -> Result<String> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("read npm package metadata: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("npm package is not a regular file: {}", path.display());
    }
    if metadata.len() > max_bytes {
        bail!("npm package exceeds the configured download size limit");
    }
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("open npm package: {}", path.display()))?,
    );
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).context("hash npm package")?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

pub fn extract_npm_package(
    package_path: &Path,
    destination: &Path,
    limits: PluginPackageLimits,
) -> Result<VerifiedPackageFiles> {
    if destination.exists() {
        bail!(
            "npm package extraction destination already exists: {}",
            destination.display()
        );
    }
    let size = fs::metadata(package_path)
        .with_context(|| format!("read npm package metadata: {}", package_path.display()))?
        .len();
    if size > limits.max_package_bytes {
        bail!("npm package exceeds the configured download size limit");
    }
    fs::create_dir_all(destination).with_context(|| {
        format!(
            "create npm package extraction destination: {}",
            destination.display()
        )
    })?;
    let result = extract_npm_package_inner(package_path, destination, limits);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn extract_npm_package_inner(
    package_path: &Path,
    destination: &Path,
    limits: PluginPackageLimits,
) -> Result<VerifiedPackageFiles> {
    let file = File::open(package_path).context("open npm tarball")?;
    let decoder = GzDecoder::new(BufReader::new(file));
    let mut archive = tar::Archive::new(decoder);
    let mut file_sha256 = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut unpacked_bytes = 0_u64;
    let mut entries = 0_usize;

    for entry in archive.entries().context("read npm tarball entries")? {
        let mut entry = entry.context("read npm tarball entry")?;
        entries = entries.saturating_add(1);
        if entries > limits.max_entries {
            bail!("npm package contains too many entries");
        }
        let entry_type = entry.header().entry_type();
        if !matches!(entry_type, EntryType::Regular | EntryType::Directory) {
            bail!("npm package contains a symlink, hard link, or special file");
        }
        let raw = entry
            .path()
            .context("read npm tarball entry path")?
            .to_string_lossy()
            .replace('\\', "/");
        let relative = npm_relative_path(raw.as_str(), entry_type.is_dir(), limits)?;
        if relative.is_empty() {
            continue;
        }
        if !seen.insert(relative.to_ascii_lowercase()) {
            bail!("npm package contains a duplicate or case-colliding path: {relative}");
        }
        let output = destination.join(relative.as_str());
        if entry_type.is_dir() {
            fs::create_dir_all(&output)
                .with_context(|| format!("create npm package directory: {}", output.display()))?;
            set_sanitized_permissions(&output, true, entry.header().mode().ok())?;
            continue;
        }
        let declared_size = entry.header().size().context("read npm entry size")?;
        if declared_size > limits.max_file_bytes {
            bail!("npm package entry exceeds the per-file size limit: {relative}");
        }
        unpacked_bytes = unpacked_bytes
            .checked_add(declared_size)
            .ok_or_else(|| anyhow!("npm package unpacked size overflow"))?;
        if unpacked_bytes > limits.max_unpacked_bytes {
            bail!("npm package exceeds the total unpacked size limit");
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create npm package parent directory: {}", parent.display())
            })?;
        }
        let mut output_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)
            .with_context(|| format!("create npm package file: {}", output.display()))?;
        let mut digest = Sha256::new();
        let mut written = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = entry
                .read(&mut buffer)
                .with_context(|| format!("read npm package entry: {relative}"))?;
            if read == 0 {
                break;
            }
            written = written
                .checked_add(read as u64)
                .ok_or_else(|| anyhow!("npm package entry size overflow"))?;
            if written > declared_size || written > limits.max_file_bytes {
                bail!("npm package entry expanded beyond its declared size: {relative}");
            }
            digest.update(&buffer[..read]);
            output_file
                .write_all(&buffer[..read])
                .with_context(|| format!("write npm package file: {}", output.display()))?;
        }
        if written != declared_size {
            bail!("npm package entry size changed while extracting: {relative}");
        }
        output_file
            .sync_all()
            .with_context(|| format!("sync npm package file: {}", output.display()))?;
        set_sanitized_permissions(&output, false, entry.header().mode().ok())?;
        file_sha256.insert(relative, hex::encode(digest.finalize()));
    }

    if !file_sha256.contains_key("package.json") {
        bail!("npm package is missing package.json");
    }
    Ok(VerifiedPackageFiles {
        root: destination.to_path_buf(),
        file_sha256,
        unpacked_bytes,
    })
}

pub fn verify_installed_file_checksums(
    root: &Path,
    expected: &BTreeMap<String, String>,
    limits: PluginPackageLimits,
) -> Result<()> {
    let mut actual = BTreeMap::new();
    let mut total_bytes = 0_u64;
    collect_installed_files(root, root, &mut actual, &mut total_bytes, limits)?;
    actual.remove(".chatos-installation.json");
    if &actual != expected {
        bail!("installed npm MCP files do not match the verified package checksums");
    }
    Ok(())
}

fn npm_relative_path(raw: &str, is_dir: bool, limits: PluginPackageLimits) -> Result<String> {
    let trimmed = raw.trim_end_matches('/');
    if trimmed == "package" && is_dir {
        return Ok(String::new());
    }
    let relative = trimmed
        .strip_prefix("package/")
        .context("npm tarball entries must be rooted under package/")?;
    normalize_package_path(relative, limits)
}

fn normalize_package_path(raw: &str, limits: PluginPackageLimits) -> Result<String> {
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.starts_with('~')
        || raw.contains('\\')
        || raw.contains('\0')
        || !raw.is_ascii()
        || raw.len() > limits.max_path_bytes
    {
        bail!("npm package contains an unsafe path: {raw:?}");
    }
    let segments = raw.split('/').collect::<Vec<_>>();
    if segments.len() > limits.max_path_depth {
        bail!("npm package path exceeds the maximum directory depth: {raw}");
    }
    for segment in &segments {
        if segment.is_empty()
            || matches!(*segment, "." | "..")
            || segment.contains(':')
            || segment.ends_with(['.', ' '])
            || segment.bytes().any(|byte| byte.is_ascii_control())
            || is_windows_reserved_name(segment)
        {
            bail!("npm package contains an unsafe path component: {segment:?}");
        }
    }
    Ok(segments.join("/"))
}

fn is_windows_reserved_name(segment: &str) -> bool {
    let base = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

#[cfg(unix)]
fn set_sanitized_permissions(path: &Path, is_dir: bool, source_mode: Option<u32>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let executable = source_mode.is_some_and(|mode| mode & 0o111 != 0);
    let mode = if is_dir || executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("set sanitized npm package permissions: {}", path.display()))
}

#[cfg(not(unix))]
fn set_sanitized_permissions(_path: &Path, _is_dir: bool, _source_mode: Option<u32>) -> Result<()> {
    Ok(())
}

fn collect_installed_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
    total_bytes: &mut u64,
    limits: PluginPackageLimits,
) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("read installed npm MCP directory: {}", directory.display()))?
    {
        let entry = entry.context("read installed npm MCP directory entry")?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(path.as_path())
            .with_context(|| format!("inspect installed npm MCP file: {}", path.display()))?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            bail!("installed npm MCP contains a symlink or special file");
        }
        if metadata.is_dir() {
            collect_installed_files(root, path.as_path(), files, total_bytes, limits)?;
            continue;
        }
        if files.len() >= limits.max_entries || metadata.len() > limits.max_file_bytes {
            bail!("installed npm MCP exceeds file count or size limits");
        }
        *total_bytes = total_bytes
            .checked_add(metadata.len())
            .ok_or_else(|| anyhow!("installed npm MCP size overflow"))?;
        if *total_bytes > limits.max_unpacked_bytes {
            bail!("installed npm MCP exceeds total size limit");
        }
        let relative = path
            .strip_prefix(root)
            .context("derive installed npm MCP relative path")?
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(
            relative,
            sha256_file(path.as_path(), limits.max_file_bytes)?,
        );
    }
    Ok(())
}

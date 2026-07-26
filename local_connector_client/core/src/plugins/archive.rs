// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginArchiveLimits {
    pub max_archive_bytes: u64,
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_unpacked_bytes: u64,
    pub max_path_bytes: usize,
    pub max_path_depth: usize,
}

impl Default for PluginArchiveLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 256 * 1024 * 1024,
            max_entries: 4_096,
            max_file_bytes: 64 * 1024 * 1024,
            max_unpacked_bytes: 512 * 1024 * 1024,
            max_path_bytes: 512,
            max_path_depth: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArchiveFiles {
    pub root: PathBuf,
    pub file_sha256: BTreeMap<String, String>,
    pub unpacked_bytes: u64,
}

#[derive(Debug)]
struct ArchiveEntryPlan {
    index: usize,
    relative_path: String,
    is_dir: bool,
    size: u64,
    unix_mode: Option<u32>,
}

pub fn sha256_file(path: &Path, max_bytes: u64) -> Result<String> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("read Plugin artifact metadata: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("Plugin artifact is not a regular file: {}", path.display());
    }
    if metadata.len() > max_bytes {
        bail!(
            "Plugin artifact exceeds the {} byte download limit",
            max_bytes
        );
    }
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("open Plugin artifact: {}", path.display()))?,
    );
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).context("hash Plugin artifact")?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

pub fn extract_plugin_archive(
    archive_path: &Path,
    destination: &Path,
    limits: PluginArchiveLimits,
) -> Result<VerifiedArchiveFiles> {
    if destination.exists() {
        bail!(
            "Plugin extraction destination already exists: {}",
            destination.display()
        );
    }
    let plan = inspect_archive(archive_path, limits)?;
    fs::create_dir_all(destination).with_context(|| {
        format!(
            "create Plugin extraction destination: {}",
            destination.display()
        )
    })?;
    match extract_planned_entries(archive_path, destination, &plan, limits) {
        Ok(files) => Ok(files),
        Err(error) => {
            let _ = fs::remove_dir_all(destination);
            Err(error)
        }
    }
}

pub fn verify_installed_file_checksums(
    root: &Path,
    expected: &BTreeMap<String, String>,
    limits: PluginArchiveLimits,
) -> Result<()> {
    let mut actual = BTreeMap::new();
    let mut total_bytes = 0_u64;
    collect_installed_files(root, root, &mut actual, &mut total_bytes, limits)?;
    actual.remove(".chatos-installation.json");
    if &actual != expected {
        bail!("installed Plugin files do not match the verified package checksums");
    }
    Ok(())
}

pub(super) fn verified_directory_files(
    root: &Path,
    limits: PluginArchiveLimits,
) -> Result<VerifiedArchiveFiles> {
    let metadata = fs::symlink_metadata(root)
        .with_context(|| format!("inspect Plugin directory: {}", root.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("Plugin directory is missing, not a directory, or is a symlink");
    }
    let mut file_sha256 = BTreeMap::new();
    let mut unpacked_bytes = 0_u64;
    collect_installed_files(root, root, &mut file_sha256, &mut unpacked_bytes, limits)?;
    Ok(VerifiedArchiveFiles {
        root: root.to_path_buf(),
        file_sha256,
        unpacked_bytes,
    })
}

pub(super) fn copy_verified_directory(
    source: &Path,
    destination: &Path,
    expected: &BTreeMap<String, String>,
    limits: PluginArchiveLimits,
) -> Result<()> {
    if destination.exists() {
        bail!(
            "Plugin copy destination already exists: {}",
            destination.display()
        );
    }
    fs::create_dir_all(destination).with_context(|| {
        format!(
            "create verified Plugin copy destination: {}",
            destination.display()
        )
    })?;
    let result = copy_directory_entries(source, source, destination, limits).and_then(|_| {
        let copied = verified_directory_files(destination, limits)?;
        if &copied.file_sha256 != expected {
            bail!("bundled Plugin changed while it was copied into isolated staging");
        }
        Ok(())
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn copy_directory_entries(
    root: &Path,
    directory: &Path,
    destination: &Path,
    limits: PluginArchiveLimits,
) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("read bundled Plugin directory: {}", directory.display()))?
    {
        let entry = entry.context("read bundled Plugin directory entry")?;
        let source_path = entry.path();
        let metadata = fs::symlink_metadata(source_path.as_path())
            .with_context(|| format!("inspect bundled Plugin file: {}", source_path.display()))?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            bail!("bundled Plugin contains a symlink or special file");
        }
        let relative = source_path
            .strip_prefix(root)
            .context("derive bundled Plugin relative path")?
            .to_string_lossy()
            .replace('\\', "/");
        let normalized = normalize_archive_path(relative.as_str(), metadata.is_dir(), limits)?;
        let output = destination.join(normalized.as_str());
        #[cfg(unix)]
        let source_mode = {
            use std::os::unix::fs::PermissionsExt;
            Some(metadata.permissions().mode())
        };
        #[cfg(not(unix))]
        let source_mode = None;
        validate_entry_type(source_mode, metadata.is_dir(), normalized.as_str())?;
        if metadata.is_dir() {
            fs::create_dir(&output).with_context(|| {
                format!("create bundled Plugin directory: {}", output.display())
            })?;
            set_sanitized_permissions(&output, true, source_mode)?;
            copy_directory_entries(root, source_path.as_path(), destination, limits)?;
        } else {
            if metadata.len() > limits.max_file_bytes {
                bail!("bundled Plugin file exceeds the configured size limit");
            }
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("create bundled Plugin parent: {}", parent.display())
                })?;
            }
            fs::copy(source_path.as_path(), output.as_path())
                .with_context(|| format!("copy bundled Plugin file: {}", source_path.display()))?;
            set_sanitized_permissions(&output, false, source_mode)?;
        }
    }
    Ok(())
}

fn inspect_archive(path: &Path, limits: PluginArchiveLimits) -> Result<Vec<ArchiveEntryPlan>> {
    let artifact_size = fs::metadata(path)
        .with_context(|| format!("read Plugin archive metadata: {}", path.display()))?
        .len();
    if artifact_size > limits.max_archive_bytes {
        bail!("Plugin archive exceeds the configured download size limit");
    }
    let file = File::open(path)
        .with_context(|| format!("open Plugin archive for inspection: {}", path.display()))?;
    let mut archive = ZipArchive::new(file).context("parse Plugin ZIP archive")?;
    if archive.len() > limits.max_entries {
        bail!("Plugin archive contains too many entries");
    }

    let mut seen = HashSet::new();
    let mut total_unpacked = 0_u64;
    let mut plan = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("inspect Plugin ZIP entry {index}"))?;
        let is_dir = entry.is_dir();
        let relative_path = normalize_archive_path(entry.name(), is_dir, limits)?;
        let collision_key = relative_path.to_ascii_lowercase();
        if !seen.insert(collision_key) {
            bail!("Plugin archive contains a duplicate or case-colliding path: {relative_path}");
        }
        validate_entry_type(entry.unix_mode(), is_dir, relative_path.as_str())?;
        if !is_dir {
            if entry.size() > limits.max_file_bytes {
                bail!("Plugin archive entry exceeds the per-file size limit: {relative_path}");
            }
            total_unpacked = total_unpacked
                .checked_add(entry.size())
                .ok_or_else(|| anyhow!("Plugin archive unpacked size overflow"))?;
            if total_unpacked > limits.max_unpacked_bytes {
                bail!("Plugin archive exceeds the total unpacked size limit");
            }
        }
        plan.push(ArchiveEntryPlan {
            index,
            relative_path,
            is_dir,
            size: entry.size(),
            unix_mode: entry.unix_mode(),
        });
    }
    Ok(plan)
}

fn extract_planned_entries(
    archive_path: &Path,
    destination: &Path,
    plan: &[ArchiveEntryPlan],
    limits: PluginArchiveLimits,
) -> Result<VerifiedArchiveFiles> {
    let file = File::open(archive_path).context("reopen verified Plugin ZIP archive")?;
    let mut archive = ZipArchive::new(file).context("parse verified Plugin ZIP archive")?;
    let mut file_sha256 = BTreeMap::new();
    let mut total_bytes = 0_u64;
    for planned in plan {
        let output = destination.join(planned.relative_path.as_str());
        if planned.is_dir {
            fs::create_dir_all(&output)
                .with_context(|| format!("create Plugin directory: {}", output.display()))?;
            set_sanitized_permissions(&output, true, planned.unix_mode)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create Plugin parent directory: {}", parent.display()))?;
        }
        let mut input = archive
            .by_index(planned.index)
            .with_context(|| format!("open Plugin ZIP entry: {}", planned.relative_path))?;
        let mut output_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)
            .with_context(|| format!("create Plugin file: {}", output.display()))?;
        let mut digest = Sha256::new();
        let mut written = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = input
                .read(&mut buffer)
                .with_context(|| format!("read Plugin ZIP entry: {}", planned.relative_path))?;
            if read == 0 {
                break;
            }
            written = written
                .checked_add(read as u64)
                .ok_or_else(|| anyhow!("Plugin archive entry size overflow"))?;
            if written > limits.max_file_bytes || written > planned.size {
                bail!(
                    "Plugin ZIP entry expanded beyond its declared size: {}",
                    planned.relative_path
                );
            }
            digest.update(&buffer[..read]);
            output_file
                .write_all(&buffer[..read])
                .with_context(|| format!("write Plugin file: {}", output.display()))?;
        }
        if written != planned.size {
            bail!(
                "Plugin ZIP entry size changed while extracting: {}",
                planned.relative_path
            );
        }
        output_file
            .sync_all()
            .with_context(|| format!("sync Plugin file: {}", output.display()))?;
        set_sanitized_permissions(&output, false, planned.unix_mode)?;
        total_bytes += written;
        file_sha256.insert(
            planned.relative_path.clone(),
            hex::encode(digest.finalize()),
        );
    }
    Ok(VerifiedArchiveFiles {
        root: destination.to_path_buf(),
        file_sha256,
        unpacked_bytes: total_bytes,
    })
}

fn normalize_archive_path(raw: &str, is_dir: bool, limits: PluginArchiveLimits) -> Result<String> {
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.starts_with('~')
        || raw.contains('\\')
        || raw.contains('\0')
        || !raw.is_ascii()
    {
        bail!("Plugin archive contains an unsafe path: {raw:?}");
    }
    let trimmed = if is_dir {
        raw.trim_end_matches('/')
    } else {
        raw
    };
    if trimmed.is_empty() || trimmed.len() > limits.max_path_bytes {
        bail!("Plugin archive path is empty or too long");
    }
    let segments = trimmed.split('/').collect::<Vec<_>>();
    if segments.len() > limits.max_path_depth {
        bail!("Plugin archive path exceeds the maximum directory depth: {trimmed}");
    }
    for segment in &segments {
        if segment.is_empty()
            || matches!(*segment, "." | "..")
            || segment.contains(':')
            || segment.ends_with(['.', ' '])
            || segment.bytes().any(|byte| byte.is_ascii_control())
            || is_windows_reserved_name(segment)
        {
            bail!("Plugin archive contains an unsafe path component: {segment:?}");
        }
    }
    Ok(segments.join("/"))
}

fn validate_entry_type(mode: Option<u32>, is_dir: bool, path: &str) -> Result<()> {
    let Some(mode) = mode else {
        return Ok(());
    };
    let kind = mode & 0o170000;
    let expected_kind = if is_dir { 0o040000 } else { 0o100000 };
    if kind != 0 && kind != expected_kind {
        bail!("Plugin archive entry is a symlink or special file: {path}");
    }
    if mode & 0o7000 != 0 {
        bail!("Plugin archive entry uses setuid, setgid, or sticky permissions: {path}");
    }
    Ok(())
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
        .with_context(|| format!("set sanitized Plugin permissions: {}", path.display()))
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
    limits: PluginArchiveLimits,
) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("read installed Plugin directory: {}", directory.display()))?
    {
        let entry = entry.context("read installed Plugin directory entry")?;
        let metadata = fs::symlink_metadata(entry.path()).with_context(|| {
            format!("inspect installed Plugin file: {}", entry.path().display())
        })?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            bail!("installed Plugin contains a symlink or special file");
        }
        if metadata.is_dir() {
            collect_installed_files(root, entry.path().as_path(), files, total_bytes, limits)?;
            continue;
        }
        if files.len() >= limits.max_entries || metadata.len() > limits.max_file_bytes {
            bail!("installed Plugin exceeds file count or size limits");
        }
        *total_bytes = total_bytes
            .checked_add(metadata.len())
            .ok_or_else(|| anyhow!("installed Plugin size overflow"))?;
        if *total_bytes > limits.max_unpacked_bytes {
            bail!("installed Plugin exceeds total size limit");
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .context("derive installed Plugin relative path")?
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(
            relative,
            sha256_file(entry.path().as_path(), limits.max_file_bytes)?,
        );
    }
    Ok(())
}

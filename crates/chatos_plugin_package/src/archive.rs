// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, parse_plugin_manifest, PluginManifest, PluginManifestSource,
    PluginReleaseRecord,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

const CODEX_MANIFEST_PATH: &str = ".codex-plugin/plugin.json";
const CHATOS_MANIFEST_PATH: &str = ".chatos-plugin/plugin.json";
const CODEX_CHECKSUMS_PATH: &str = ".codex-plugin/checksums.json";
const CHATOS_CHECKSUMS_PATH: &str = ".chatos-plugin/checksums.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginPackageLimits {
    pub max_archive_bytes: usize,
    pub max_entries: usize,
    pub max_file_bytes: usize,
    pub max_unpacked_bytes: usize,
    pub max_path_bytes: usize,
    pub max_path_depth: usize,
}

impl Default for PluginPackageLimits {
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
pub struct VerifiedPluginPackage {
    pub manifest: PluginManifest,
    pub manifest_source: PluginManifestSource,
    pub artifact_sha256: String,
    pub file_sha256: BTreeMap<String, String>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub unpacked_bytes: usize,
}

#[derive(Debug, Error)]
pub enum PluginPackageError {
    #[error("{0}")]
    Invalid(String),
    #[error("parse Plugin ZIP failed: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("read Plugin ZIP failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse Plugin package JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginChecksumIndex {
    schema_version: u32,
    files: BTreeMap<String, String>,
}

pub fn verify_plugin_archive_bytes(
    bytes: &[u8],
    release: &PluginReleaseRecord,
    limits: PluginPackageLimits,
) -> Result<VerifiedPluginPackage, PluginPackageError> {
    if bytes.len() > limits.max_archive_bytes {
        return invalid("Plugin artifact exceeds the archive size limit");
    }
    let artifact_sha256 = sha256(bytes);
    if artifact_sha256 != release.artifact_sha256 {
        return invalid("Plugin artifact SHA-256 does not match the signed Release");
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > limits.max_entries {
        return invalid("Plugin archive contains too many entries");
    }
    let mut files = BTreeMap::new();
    let mut file_sha256 = BTreeMap::new();
    let mut collision_keys = HashSet::new();
    let mut unpacked_bytes = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let is_dir = entry.is_dir();
        let path = normalized_archive_path(entry.name(), is_dir, limits)?;
        if !collision_keys.insert(path.to_ascii_lowercase()) {
            return invalid(format!(
                "Plugin archive contains a duplicate or case-colliding path: {path}"
            ));
        }
        validate_entry_mode(entry.unix_mode(), is_dir, path.as_str())?;
        if is_dir {
            continue;
        }
        let declared_size = usize::try_from(entry.size())
            .map_err(|_| PluginPackageError::Invalid("Plugin file size overflow".to_string()))?;
        if declared_size > limits.max_file_bytes {
            return invalid(format!("Plugin file exceeds the size limit: {path}"));
        }
        unpacked_bytes = unpacked_bytes
            .checked_add(declared_size)
            .ok_or_else(|| PluginPackageError::Invalid("Plugin size overflow".to_string()))?;
        if unpacked_bytes > limits.max_unpacked_bytes {
            return invalid("Plugin archive exceeds the unpacked size limit");
        }
        let mut body = Vec::with_capacity(declared_size);
        entry.read_to_end(&mut body)?;
        if body.len() != declared_size {
            return invalid(format!("Plugin file size changed while reading: {path}"));
        }
        file_sha256.insert(path.clone(), sha256(body.as_slice()));
        files.insert(path, body);
    }

    verified_release_package(files, file_sha256, unpacked_bytes, artifact_sha256, release)
}

/// Verifies a compile-time embedded Plugin package file set against an immutable Release.
///
/// The Release artifact hash remains the identity of the separately reproducible ZIP. This
/// function verifies the embedded source file set, exact checksum index, Manifest, and SBOM so a
/// bundled cloud runtime can materialize the same canonical component Bundles without network I/O.
pub fn verify_embedded_plugin_package_files(
    files: BTreeMap<String, Vec<u8>>,
    release: &PluginReleaseRecord,
    limits: PluginPackageLimits,
) -> Result<VerifiedPluginPackage, PluginPackageError> {
    if files.len() > limits.max_entries {
        return invalid("embedded Plugin package contains too many files");
    }
    if !is_sha256(release.artifact_sha256.as_str()) {
        return invalid("embedded Plugin Release artifact SHA-256 is invalid");
    }
    let mut normalized_files = BTreeMap::new();
    let mut file_sha256 = BTreeMap::new();
    let mut collision_keys = HashSet::new();
    let mut unpacked_bytes = 0usize;
    for (raw_path, body) in files {
        let path = normalized_archive_path(raw_path.as_str(), false, limits)?;
        if path != raw_path {
            return invalid(format!("embedded Plugin path is not canonical: {raw_path}"));
        }
        if !collision_keys.insert(path.to_ascii_lowercase()) {
            return invalid(format!(
                "embedded Plugin package contains a duplicate or case-colliding path: {path}"
            ));
        }
        if body.len() > limits.max_file_bytes {
            return invalid(format!(
                "embedded Plugin file exceeds the size limit: {path}"
            ));
        }
        unpacked_bytes = unpacked_bytes
            .checked_add(body.len())
            .ok_or_else(|| PluginPackageError::Invalid("Plugin size overflow".to_string()))?;
        if unpacked_bytes > limits.max_unpacked_bytes {
            return invalid("embedded Plugin package exceeds the unpacked size limit");
        }
        file_sha256.insert(path.clone(), sha256(body.as_slice()));
        normalized_files.insert(path, body);
    }
    verified_release_package(
        normalized_files,
        file_sha256,
        unpacked_bytes,
        release.artifact_sha256.clone(),
        release,
    )
}

fn verified_release_package(
    files: BTreeMap<String, Vec<u8>>,
    file_sha256: BTreeMap<String, String>,
    unpacked_bytes: usize,
    artifact_sha256: String,
    release: &PluginReleaseRecord,
) -> Result<VerifiedPluginPackage, PluginPackageError> {
    let (manifest_path, manifest_source, checksum_path) = metadata_paths(&files)?;
    let manifest_raw = utf8_file(&files, manifest_path, 1024 * 1024, "Plugin Manifest")?;
    let manifest = parse_plugin_manifest(manifest_raw, manifest_source)
        .map_err(|error| PluginPackageError::Invalid(error.to_string()))?;
    if manifest != release.normalized_manifest
        || release.manifest_schema_version != manifest.schema_version
        || release.version != manifest.version
    {
        return invalid("Plugin package Manifest does not match the signed Release");
    }
    verify_checksums(&files, &file_sha256, checksum_path)?;
    verify_sbom(&files, release)?;
    Ok(VerifiedPluginPackage {
        manifest,
        manifest_source,
        artifact_sha256,
        file_sha256,
        files,
        unpacked_bytes,
    })
}

pub fn load_verified_plugin_package_directory(
    root: &Path,
    artifact_sha256: &str,
    expected_file_sha256: &BTreeMap<String, String>,
    limits: PluginPackageLimits,
) -> Result<VerifiedPluginPackage, PluginPackageError> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return invalid("installed Plugin root is missing, unsafe, or not a directory");
    }
    let mut files = BTreeMap::new();
    let mut file_sha256 = BTreeMap::new();
    let mut collision_keys = HashSet::new();
    let mut unpacked_bytes = 0usize;
    let mut entries = 0usize;
    collect_directory_files(
        root,
        root,
        limits,
        &mut entries,
        &mut unpacked_bytes,
        &mut collision_keys,
        &mut files,
        &mut file_sha256,
    )?;
    if &file_sha256 != expected_file_sha256 {
        return invalid("installed Plugin files do not match the verified package checksums");
    }
    let (manifest_path, manifest_source, checksum_path) = metadata_paths(&files)?;
    let manifest_raw = utf8_file(&files, manifest_path, 1024 * 1024, "Plugin Manifest")?;
    let manifest = parse_plugin_manifest(manifest_raw, manifest_source)
        .map_err(|error| PluginPackageError::Invalid(error.to_string()))?;
    verify_checksums(&files, &file_sha256, checksum_path)?;
    Ok(VerifiedPluginPackage {
        manifest,
        manifest_source,
        artifact_sha256: artifact_sha256.to_string(),
        file_sha256,
        files,
        unpacked_bytes,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_directory_files(
    root: &Path,
    directory: &Path,
    limits: PluginPackageLimits,
    entries: &mut usize,
    unpacked_bytes: &mut usize,
    collision_keys: &mut HashSet<String>,
    files: &mut BTreeMap<String, Vec<u8>>,
    file_sha256: &mut BTreeMap<String, String>,
) -> Result<(), PluginPackageError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(path.as_path())?;
        let is_dir = metadata.is_dir();
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !is_dir) {
            return invalid("installed Plugin contains a symlink or special file");
        }
        let raw = path
            .strip_prefix(root)
            .map_err(|_| PluginPackageError::Invalid("derive installed Plugin path failed".into()))?
            .to_str()
            .ok_or_else(|| {
                PluginPackageError::Invalid("installed Plugin path is not UTF-8".into())
            })?
            .replace('\\', "/");
        if raw == ".chatos-installation.json" && metadata.is_file() {
            continue;
        }
        *entries = entries
            .checked_add(1)
            .ok_or_else(|| PluginPackageError::Invalid("Plugin entry count overflow".into()))?;
        if *entries > limits.max_entries {
            return invalid("installed Plugin contains too many entries");
        }
        let normalized = normalized_archive_path(raw.as_str(), is_dir, limits)?;
        if !collision_keys.insert(normalized.to_ascii_lowercase()) {
            return invalid(format!(
                "installed Plugin contains a duplicate or case-colliding path: {normalized}"
            ));
        }
        validate_entry_mode(directory_entry_mode(&metadata), is_dir, normalized.as_str())?;
        if is_dir {
            collect_directory_files(
                root,
                path.as_path(),
                limits,
                entries,
                unpacked_bytes,
                collision_keys,
                files,
                file_sha256,
            )?;
            continue;
        }
        let size = usize::try_from(metadata.len())
            .map_err(|_| PluginPackageError::Invalid("Plugin file size overflow".into()))?;
        if size > limits.max_file_bytes {
            return invalid(format!(
                "installed Plugin file exceeds the size limit: {normalized}"
            ));
        }
        *unpacked_bytes = unpacked_bytes
            .checked_add(size)
            .ok_or_else(|| PluginPackageError::Invalid("Plugin size overflow".into()))?;
        if *unpacked_bytes > limits.max_unpacked_bytes {
            return invalid("installed Plugin exceeds the unpacked size limit");
        }
        let body = fs::read(path.as_path())?;
        if body.len() != size {
            return invalid(format!(
                "installed Plugin file changed while reading: {normalized}"
            ));
        }
        file_sha256.insert(normalized.clone(), sha256(body.as_slice()));
        files.insert(normalized, body);
    }
    Ok(())
}

#[cfg(unix)]
fn directory_entry_mode(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode())
}

#[cfg(not(unix))]
fn directory_entry_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

fn metadata_paths(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(&'static str, PluginManifestSource, &'static str), PluginPackageError> {
    match (
        files.contains_key(CODEX_MANIFEST_PATH),
        files.contains_key(CHATOS_MANIFEST_PATH),
    ) {
        (true, false) if files.contains_key(CODEX_CHECKSUMS_PATH) => Ok((
            CODEX_MANIFEST_PATH,
            PluginManifestSource::Codex,
            CODEX_CHECKSUMS_PATH,
        )),
        (false, true) if files.contains_key(CHATOS_CHECKSUMS_PATH) => Ok((
            CHATOS_MANIFEST_PATH,
            PluginManifestSource::Chatos,
            CHATOS_CHECKSUMS_PATH,
        )),
        (true, true) => invalid("Plugin archive contains both root Manifests"),
        (false, false) => invalid("Plugin archive is missing a root Manifest"),
        _ => invalid("Plugin archive is missing its checksum index"),
    }
}

fn verify_checksums(
    files: &BTreeMap<String, Vec<u8>>,
    actual: &BTreeMap<String, String>,
    checksum_path: &str,
) -> Result<(), PluginPackageError> {
    let raw = utf8_file(files, checksum_path, 1024 * 1024, "Plugin checksum index")?;
    let index: PluginChecksumIndex = serde_json::from_str(raw)?;
    if index.schema_version != 1 {
        return invalid("unsupported Plugin checksum index schema version");
    }
    let mut expected = BTreeMap::new();
    for (path, digest) in index.files {
        let normalized = normalize_plugin_relative_path(path.as_str())
            .map_err(PluginPackageError::Invalid)?
            .trim_start_matches("./")
            .to_string();
        if normalized == checksum_path || expected.insert(normalized, digest).is_some() {
            return invalid("Plugin checksum index contains duplicate or self-referential paths");
        }
    }
    let mut covered = actual.clone();
    covered.remove(checksum_path);
    if expected.keys().ne(covered.keys()) {
        return invalid("Plugin checksum index must cover every package file exactly once");
    }
    if covered.iter().any(|(path, digest)| {
        expected
            .get(path)
            .is_none_or(|value| value != digest || !is_sha256(value))
    }) {
        return invalid("Plugin checksum index contains a mismatched digest");
    }
    Ok(())
}

fn verify_sbom(
    files: &BTreeMap<String, Vec<u8>>,
    release: &PluginReleaseRecord,
) -> Result<(), PluginPackageError> {
    let reference = release
        .sbom_ref
        .as_deref()
        .ok_or_else(|| PluginPackageError::Invalid("Plugin Release must embed an SBOM".into()))?;
    if reference.contains("://") {
        return invalid("Plugin SBOM must be embedded in the artifact");
    }
    let path = normalize_plugin_relative_path(reference)
        .map_err(PluginPackageError::Invalid)?
        .trim_start_matches("./")
        .to_string();
    let raw = utf8_file(files, path.as_str(), 4 * 1024 * 1024, "Plugin SBOM")?;
    let document: serde_json::Value = serde_json::from_str(raw)?;
    let spdx = document
        .get("spdxVersion")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.starts_with("SPDX-"))
        && document
            .get("SPDXID")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
    let cyclonedx = document
        .get("bomFormat")
        .and_then(serde_json::Value::as_str)
        == Some("CycloneDX")
        && document
            .get("specVersion")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
    if !spdx && !cyclonedx {
        return invalid("Plugin SBOM must be SPDX JSON or CycloneDX JSON");
    }
    Ok(())
}

fn utf8_file<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    path: &str,
    max_bytes: usize,
    label: &str,
) -> Result<&'a str, PluginPackageError> {
    let bytes = files
        .get(path)
        .ok_or_else(|| PluginPackageError::Invalid(format!("{label} is missing")))?;
    if bytes.len() > max_bytes || bytes.contains(&0) {
        return invalid(format!("{label} exceeds its limit or contains NUL"));
    }
    std::str::from_utf8(bytes)
        .map_err(|_| PluginPackageError::Invalid(format!("{label} is not UTF-8")))
}

fn normalized_archive_path(
    raw: &str,
    is_dir: bool,
    limits: PluginPackageLimits,
) -> Result<String, PluginPackageError> {
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.starts_with('~')
        || raw.contains(['\\', '\0'])
        || !raw.is_ascii()
    {
        return invalid(format!("Plugin archive contains an unsafe path: {raw:?}"));
    }
    let trimmed = if is_dir {
        raw.trim_end_matches('/')
    } else {
        raw
    };
    if trimmed.is_empty()
        || trimmed.len() > limits.max_path_bytes
        || trimmed.split('/').count() > limits.max_path_depth
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return invalid(format!("Plugin archive contains an unsafe path: {raw:?}"));
    }
    Ok(trimmed.to_string())
}

fn validate_entry_mode(
    mode: Option<u32>,
    is_dir: bool,
    path: &str,
) -> Result<(), PluginPackageError> {
    let Some(mode) = mode else { return Ok(()) };
    let file_type = mode & 0o170000;
    let expected = if is_dir { 0o040000 } else { 0o100000 };
    if file_type != 0 && file_type != expected {
        return invalid(format!(
            "Plugin archive entry is a symlink or special file: {path}"
        ));
    }
    if mode & 0o7000 != 0 {
        return invalid(format!(
            "Plugin archive entry has unsafe permission bits: {path}"
        ));
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn invalid<T>(message: impl Into<String>) -> Result<T, PluginPackageError> {
    Err(PluginPackageError::Invalid(message.into()))
}

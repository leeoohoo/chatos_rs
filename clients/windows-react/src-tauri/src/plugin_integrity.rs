use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const INTEGRITY_SCHEMA_VERSION: u32 = 1;
const INTEGRITY_DIRECTORY: &str = ".integrity";
const MAXIMUM_FILES: usize = 20_000;
const MAXIMUM_TOTAL_BYTES: u64 = 512 * 1_024 * 1_024;
const MAXIMUM_RECORD_BYTES: u64 = 16 * 1_024 * 1_024;
const COMPONENT_REVISION_PREFIX: &str = "integrity-v1-";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallationIntegrityRecord {
    schema_version: u32,
    artifact_sha256: String,
    installation_relative_path: String,
    files: BTreeMap<String, String>,
}

pub(crate) fn secure_component_revision(artifact_sha256: &str) -> Result<String, String> {
    normalized_sha256(artifact_sha256).map(|digest| format!("{COMPONENT_REVISION_PREFIX}{digest}"))
}

pub(crate) fn requires_installation_integrity(component_revision: &str) -> bool {
    component_revision.starts_with(COMPONENT_REVISION_PREFIX)
}

pub(crate) fn write_installation_integrity(
    plugin_root: &Path,
    installation_root: &Path,
    artifact_sha256: &str,
) -> Result<(), String> {
    let artifact_sha256 = normalized_sha256(artifact_sha256)?;
    let plugin_root = canonical_directory(plugin_root)?;
    let installation_root = canonical_directory(installation_root)?;
    if installation_root == plugin_root || !installation_root.starts_with(&plugin_root) {
        return Err(integrity_error());
    }
    let relative = installation_root
        .strip_prefix(&plugin_root)
        .map_err(|_| integrity_error())?;
    let record = InstallationIntegrityRecord {
        schema_version: INTEGRITY_SCHEMA_VERSION,
        artifact_sha256: artifact_sha256.clone(),
        installation_relative_path: portable_relative_path(relative)?,
        files: snapshot_tree(&installation_root)?,
    };
    let bytes = serde_json::to_vec(&record).map_err(|_| integrity_error())?;
    if bytes.is_empty() || bytes.len() as u64 > MAXIMUM_RECORD_BYTES {
        return Err(integrity_error());
    }
    let directory = plugin_root.join(INTEGRITY_DIRECTORY);
    fs::create_dir_all(&directory).map_err(|_| integrity_error())?;
    let directory = canonical_directory(&directory)?;
    if !directory.starts_with(&plugin_root) {
        return Err(integrity_error());
    }
    let destination = directory.join(format!("{artifact_sha256}.json"));
    let temporary = directory.join(format!(".{artifact_sha256}-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| integrity_error())?;
        file.write_all(&bytes).map_err(|_| integrity_error())?;
        file.sync_all().map_err(|_| integrity_error())?;
        if destination.exists() {
            fs::remove_file(&destination).map_err(|_| integrity_error())?;
        }
        fs::rename(&temporary, &destination).map_err(|_| integrity_error())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(crate) fn verify_installation_integrity(
    plugin_root: &Path,
    installation_root: &Path,
    release_digest: &str,
) -> Result<(), String> {
    let artifact_sha256 = normalized_sha256(release_digest)?;
    let plugin_root = canonical_directory(plugin_root)?;
    let installation_root = canonical_directory(installation_root)?;
    if installation_root == plugin_root || !installation_root.starts_with(&plugin_root) {
        return Err(integrity_error());
    }
    let record_path = plugin_root
        .join(INTEGRITY_DIRECTORY)
        .join(format!("{artifact_sha256}.json"));
    let metadata = fs::symlink_metadata(&record_path).map_err(|_| integrity_error())?;
    if !metadata.is_file()
        || is_symlink_or_reparse(&metadata)
        || metadata.len() == 0
        || metadata.len() > MAXIMUM_RECORD_BYTES
    {
        return Err(integrity_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(&record_path)
        .and_then(|file| file.take(MAXIMUM_RECORD_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| integrity_error())?;
    if bytes.len() as u64 > MAXIMUM_RECORD_BYTES {
        return Err(integrity_error());
    }
    let record: InstallationIntegrityRecord =
        serde_json::from_slice(&bytes).map_err(|_| integrity_error())?;
    let relative = installation_root
        .strip_prefix(&plugin_root)
        .map_err(|_| integrity_error())?;
    if record.schema_version != INTEGRITY_SCHEMA_VERSION
        || record.artifact_sha256 != artifact_sha256
        || record.installation_relative_path != portable_relative_path(relative)?
        || record.files.is_empty()
        || record.files != snapshot_tree(&installation_root)?
    {
        return Err(integrity_error());
    }
    Ok(())
}

pub(crate) fn remove_installation_integrity(plugin_root: &Path, release_digest: &str) {
    let Ok(artifact_sha256) = normalized_sha256(release_digest) else {
        return;
    };
    let _ = fs::remove_file(
        plugin_root
            .join(INTEGRITY_DIRECTORY)
            .join(format!("{artifact_sha256}.json")),
    );
}

fn snapshot_tree(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut directories = vec![root.to_path_buf()];
    let mut files = BTreeMap::new();
    let mut total_bytes = 0_u64;
    while let Some(directory) = directories.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(|_| integrity_error())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| integrity_error())?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|_| integrity_error())?;
            if is_symlink_or_reparse(&metadata) {
                return Err(integrity_error());
            }
            if metadata.is_dir() {
                directories.push(path);
                continue;
            }
            if !metadata.is_file() {
                return Err(integrity_error());
            }
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .filter(|value| *value <= MAXIMUM_TOTAL_BYTES)
                .ok_or_else(integrity_error)?;
            if files.len() >= MAXIMUM_FILES {
                return Err(integrity_error());
            }
            let relative = path.strip_prefix(root).map_err(|_| integrity_error())?;
            let key = portable_relative_path(relative)?;
            if files.insert(key, sha256_file(&path)?).is_some() {
                return Err(integrity_error());
            }
        }
    }
    if files.is_empty() {
        return Err(integrity_error());
    }
    Ok(files)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| integrity_error())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|_| integrity_error())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| integrity_error())?;
    if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
        return Err(integrity_error());
    }
    path.canonicalize().map_err(|_| integrity_error())
}

fn portable_relative_path(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(integrity_error());
        };
        let component = component.to_str().ok_or_else(integrity_error)?;
        if component.is_empty() || component.contains(['/', '\\', '\0']) {
            return Err(integrity_error());
        }
        parts.push(component);
    }
    (!parts.is_empty())
        .then(|| parts.join("/"))
        .ok_or_else(integrity_error)
}

fn normalized_sha256(value: &str) -> Result<String, String> {
    let value = value.trim().strip_prefix("sha256:").unwrap_or(value.trim());
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(integrity_error());
    }
    Ok(value.to_owned())
}

#[cfg(windows)]
fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn integrity_error() -> String {
    "Plugin 安装文件未通过完整性校验。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_record_detects_file_changes_and_extra_files() {
        let temporary = tempfile::tempdir().unwrap();
        let plugin_root = temporary.path().join("Plugins");
        let installation = plugin_root.join("plugin").join("1.0.0");
        fs::create_dir_all(&installation).unwrap();
        fs::write(installation.join("chatos.plugin.json"), b"original").unwrap();
        let digest = "a".repeat(64);
        write_installation_integrity(&plugin_root, &installation, &digest).unwrap();
        verify_installation_integrity(&plugin_root, &installation, &digest).unwrap();

        fs::write(installation.join("chatos.plugin.json"), b"tampered").unwrap();
        assert!(verify_installation_integrity(&plugin_root, &installation, &digest).is_err());
        fs::write(installation.join("chatos.plugin.json"), b"original").unwrap();
        fs::write(installation.join("unexpected.txt"), b"extra").unwrap();
        assert!(verify_installation_integrity(&plugin_root, &installation, &digest).is_err());
    }

    #[test]
    fn secure_component_revision_marks_records_that_require_integrity() {
        let revision = secure_component_revision(&"b".repeat(64)).unwrap();
        assert!(requires_installation_integrity(&revision));
        assert!(!requires_installation_integrity("legacy-revision"));
    }
}

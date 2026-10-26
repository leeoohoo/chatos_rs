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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallationIntegrityRecord {
    schema_version: u32,
    artifact_sha256: String,
    installation_relative_path: String,
    files: BTreeMap<String, String>,
}

struct BackupRecoveryCandidate {
    path: PathBuf,
    installation_relative_path: String,
    artifact_sha256: Vec<String>,
    consumed: bool,
}

pub(crate) fn recover_interrupted_installations(
    plugin_root: &Path,
    expected_installations: &[(PathBuf, String)],
) -> Result<usize, String> {
    if expected_installations.is_empty() {
        return Ok(0);
    }
    let requested_plugin_root = plugin_root.to_path_buf();
    let plugin_root = canonical_directory(plugin_root)?;
    let mut expected = BTreeMap::<String, (PathBuf, String)>::new();
    for (installation_root, release_digest) in expected_installations {
        let installation_root = if installation_root.starts_with(&plugin_root) {
            installation_root.clone()
        } else {
            let relative = installation_root
                .strip_prefix(&requested_plugin_root)
                .map_err(|_| integrity_error())?;
            plugin_root.join(relative)
        };
        let relative = validate_expected_installation_path(&plugin_root, &installation_root)?;
        let digest = normalized_sha256(release_digest)?;
        match expected.get(&relative) {
            Some((_, current)) if current != &digest => return Err(integrity_error()),
            Some(_) => {}
            None => {
                expected.insert(relative, (installation_root, digest));
            }
        }
    }
    let records = load_integrity_records(&plugin_root)?;
    let mut candidates = load_backup_recovery_candidates(&plugin_root, &records)?;
    let mut recovered = 0usize;
    for (relative, (installation_root, expected_digest)) in expected {
        if verify_installation_integrity(&plugin_root, &installation_root, &expected_digest).is_ok()
        {
            for candidate in candidates.iter_mut().filter(|candidate| {
                !candidate.consumed && candidate.installation_relative_path == relative
            }) {
                if quarantine_backup(&plugin_root, &candidate.path, "obsolete-backup").is_ok() {
                    candidate.consumed = true;
                    recovered = recovered.saturating_add(1);
                }
            }
            continue;
        }

        let matching = candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                !candidate.consumed
                    && candidate.installation_relative_path == relative
                    && candidate
                        .artifact_sha256
                        .iter()
                        .any(|digest| digest == &expected_digest)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            continue;
        }
        let index = matching[0];
        restore_backup(
            &plugin_root,
            &candidates[index].path,
            &installation_root,
            &expected_digest,
        )?;
        candidates[index].consumed = true;
        recovered = recovered.saturating_add(1);
    }
    remove_empty_backup_directory(&plugin_root);
    Ok(recovered)
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

fn load_integrity_records(plugin_root: &Path) -> Result<Vec<InstallationIntegrityRecord>, String> {
    let directory = plugin_root.join(INTEGRITY_DIRECTORY);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let directory = canonical_confined_directory(plugin_root, &directory)?;
    let entries = fs::read_dir(directory).map_err(|_| integrity_error())?;
    let mut records = Vec::new();
    for (visited, entry) in entries.enumerate() {
        if visited >= MAXIMUM_FILES {
            return Err(integrity_error());
        }
        let entry = entry.map_err(|_| integrity_error())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(digest) = name.strip_suffix(".json") else {
            continue;
        };
        if normalized_sha256(digest).is_err() {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| integrity_error())?;
        if !metadata.is_file()
            || is_symlink_or_reparse(&metadata)
            || metadata.len() == 0
            || metadata.len() > MAXIMUM_RECORD_BYTES
        {
            continue;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(entry.path())
            .and_then(|file| file.take(MAXIMUM_RECORD_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|_| integrity_error())?;
        let Ok(record) = serde_json::from_slice::<InstallationIntegrityRecord>(&bytes) else {
            continue;
        };
        if record.schema_version == INTEGRITY_SCHEMA_VERSION
            && record.artifact_sha256 == digest
            && normalized_relative_path(&record.installation_relative_path).is_some()
            && !record.files.is_empty()
        {
            records.push(record);
        }
    }
    Ok(records)
}

fn load_backup_recovery_candidates(
    plugin_root: &Path,
    records: &[InstallationIntegrityRecord],
) -> Result<Vec<BackupRecoveryCandidate>, String> {
    let backup_directory = plugin_root.join(".backups");
    if !backup_directory.exists() {
        return Ok(Vec::new());
    }
    let backup_directory = canonical_confined_directory(plugin_root, &backup_directory)?;
    let entries = fs::read_dir(&backup_directory).map_err(|_| integrity_error())?;
    let mut candidates = Vec::new();
    for (visited, entry) in entries.enumerate() {
        if visited >= MAXIMUM_FILES {
            return Err(integrity_error());
        }
        let entry = entry.map_err(|_| integrity_error())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !uuid::Uuid::parse_str(&name).is_ok_and(|value| value.to_string() == name) {
            continue;
        }
        let Ok(path) = canonical_confined_directory(&backup_directory, &entry.path()) else {
            continue;
        };
        let Ok(files) = snapshot_tree(&path) else {
            continue;
        };
        let matching = records
            .iter()
            .filter(|record| record.files == files)
            .collect::<Vec<_>>();
        let mut relative_paths = matching
            .iter()
            .map(|record| record.installation_relative_path.clone())
            .collect::<Vec<_>>();
        relative_paths.sort();
        relative_paths.dedup();
        if relative_paths.len() != 1 {
            continue;
        }
        let mut artifact_sha256 = matching
            .into_iter()
            .map(|record| record.artifact_sha256.clone())
            .collect::<Vec<_>>();
        artifact_sha256.sort();
        artifact_sha256.dedup();
        candidates.push(BackupRecoveryCandidate {
            path,
            installation_relative_path: relative_paths.remove(0),
            artifact_sha256,
            consumed: false,
        });
    }
    Ok(candidates)
}

fn validate_expected_installation_path(
    plugin_root: &Path,
    installation_root: &Path,
) -> Result<String, String> {
    let relative = installation_root
        .strip_prefix(plugin_root)
        .map_err(|_| integrity_error())?;
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().ok_or_else(integrity_error),
            _ => Err(integrity_error()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.len() != 2
        || components
            .iter()
            .any(|component| normalized_sha256(component).is_err())
    {
        return Err(integrity_error());
    }
    let parent = plugin_root.join(components[0]);
    if parent.exists() {
        canonical_confined_directory(plugin_root, &parent)?;
    }
    if installation_root.exists() {
        canonical_confined_directory(plugin_root, installation_root)?;
    }
    portable_relative_path(relative)
}

fn restore_backup(
    plugin_root: &Path,
    backup: &Path,
    installation_root: &Path,
    expected_digest: &str,
) -> Result<(), String> {
    let displaced = if installation_root.exists() {
        Some(quarantine_tree(
            plugin_root,
            installation_root,
            "displaced-installation",
        )?)
    } else {
        None
    };
    if fs::rename(backup, installation_root).is_err() {
        if let Some(displaced) = &displaced {
            let _ = fs::rename(displaced, installation_root);
        }
        return Err(integrity_error());
    }
    if verify_installation_integrity(plugin_root, installation_root, expected_digest).is_err() {
        let _ = fs::rename(installation_root, backup);
        if let Some(displaced) = &displaced {
            let _ = fs::rename(displaced, installation_root);
        }
        return Err(integrity_error());
    }
    Ok(())
}

fn quarantine_backup(plugin_root: &Path, backup: &Path, name: &str) -> Result<(), String> {
    quarantine_tree(plugin_root, backup, name).map(|_| ())
}

fn quarantine_tree(plugin_root: &Path, source: &Path, name: &str) -> Result<PathBuf, String> {
    let staging = plugin_root.join(".staging");
    fs::create_dir_all(&staging).map_err(|_| integrity_error())?;
    let staging = canonical_confined_directory(plugin_root, &staging)?;
    let container = staging.join(uuid::Uuid::new_v4().to_string());
    fs::create_dir(&container).map_err(|_| integrity_error())?;
    let destination = container.join(name);
    if fs::rename(source, &destination).is_err() {
        let _ = fs::remove_dir(&container);
        return Err(integrity_error());
    }
    Ok(destination)
}

fn remove_empty_backup_directory(plugin_root: &Path) {
    let directory = plugin_root.join(".backups");
    if fs::read_dir(&directory)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
    {
        let _ = fs::remove_dir(directory);
    }
}

fn canonical_confined_directory(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| integrity_error())?;
    if !metadata.is_dir() || is_symlink_or_reparse(&metadata) {
        return Err(integrity_error());
    }
    let canonical = path.canonicalize().map_err(|_| integrity_error())?;
    if canonical == root || !canonical.starts_with(root) {
        return Err(integrity_error());
    }
    Ok(canonical)
}

fn normalized_relative_path(value: &str) -> Option<PathBuf> {
    let mut path = PathBuf::new();
    let mut count = 0usize;
    for component in value.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.contains(['\\', '\0'])
        {
            return None;
        }
        path.push(component);
        count = count.saturating_add(1);
    }
    (count > 0).then_some(path)
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

    #[test]
    fn recovery_restores_the_backup_when_host_metadata_still_selects_it() {
        let temporary = tempfile::tempdir().unwrap();
        let plugin_root = temporary.path().join("Plugins");
        let installation = plugin_root.join("1".repeat(64)).join("2".repeat(64));
        fs::create_dir_all(&installation).unwrap();
        fs::write(installation.join("payload.txt"), b"old").unwrap();
        let old_digest = "a".repeat(64);
        let new_digest = "b".repeat(64);
        write_installation_integrity(&plugin_root, &installation, &old_digest).unwrap();

        let backup = plugin_root
            .join(".backups")
            .join("00000000-0000-0000-0000-000000000000");
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::rename(&installation, &backup).unwrap();
        fs::create_dir_all(&installation).unwrap();
        fs::write(installation.join("payload.txt"), b"new").unwrap();
        write_installation_integrity(&plugin_root, &installation, &new_digest).unwrap();

        assert_eq!(
            recover_interrupted_installations(
                &plugin_root,
                &[(installation.clone(), format!("sha256:{old_digest}"))],
            )
            .unwrap(),
            1
        );
        assert_eq!(fs::read(installation.join("payload.txt")).unwrap(), b"old");
        verify_installation_integrity(&plugin_root, &installation, &old_digest).unwrap();
        assert!(!plugin_root.join(".backups").exists());
        assert!(fs::read_dir(plugin_root.join(".staging"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .path()
                .join("displaced-installation")
                .is_dir()));
    }

    #[test]
    fn recovery_with_no_host_installations_does_not_create_plugin_storage() {
        let temporary = tempfile::tempdir().unwrap();
        let plugin_root = temporary.path().join("Plugins");
        assert_eq!(
            recover_interrupted_installations(&plugin_root, &[]).unwrap(),
            0
        );
        assert!(!plugin_root.exists());
    }

    #[test]
    fn recovery_keeps_the_new_installation_when_host_metadata_committed_it() {
        let temporary = tempfile::tempdir().unwrap();
        let plugin_root = temporary.path().join("Plugins");
        let installation = plugin_root.join("3".repeat(64)).join("4".repeat(64));
        fs::create_dir_all(&installation).unwrap();
        fs::write(installation.join("payload.txt"), b"old").unwrap();
        let old_digest = "c".repeat(64);
        let new_digest = "d".repeat(64);
        write_installation_integrity(&plugin_root, &installation, &old_digest).unwrap();

        let backup = plugin_root
            .join(".backups")
            .join("11111111-1111-1111-1111-111111111111");
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::rename(&installation, &backup).unwrap();
        fs::create_dir_all(&installation).unwrap();
        fs::write(installation.join("payload.txt"), b"new").unwrap();
        write_installation_integrity(&plugin_root, &installation, &new_digest).unwrap();

        assert_eq!(
            recover_interrupted_installations(
                &plugin_root,
                &[(installation.clone(), format!("sha256:{new_digest}"))],
            )
            .unwrap(),
            1
        );
        assert_eq!(fs::read(installation.join("payload.txt")).unwrap(), b"new");
        verify_installation_integrity(&plugin_root, &installation, &new_digest).unwrap();
        assert!(!plugin_root.join(".backups").exists());
        assert!(fs::read_dir(plugin_root.join(".staging"))
            .unwrap()
            .any(|entry| entry.unwrap().path().join("obsolete-backup").is_dir()));
    }
}

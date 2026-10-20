use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

use super::models::ToolApprovalDecision;

const MAXIMUM_ENTRIES: usize = 256;
const MAXIMUM_RECORD_BYTES: u64 = 16 * 1_024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordedToolApprovalDecision {
    pub owner_user_id: String,
    pub invocation_id: String,
    pub invocation_version: u64,
    pub decision: ToolApprovalDecision,
    pub decided_by: String,
    pub reason: String,
    pub recorded_at_unix_ms: i64,
}

impl RecordedToolApprovalDecision {
    pub fn matches(&self, owner_user_id: &str, invocation_id: &str, version: u64) -> bool {
        self.owner_user_id == owner_user_id
            && self.invocation_id == invocation_id
            && self.invocation_version == version
    }

    fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("invocation_id", &self.invocation_id)?;
        if self.invocation_version == 0 {
            return Err("invocation_version must be greater than zero".into());
        }
        validate_identifier("decided_by", &self.decided_by)?;
        if self.reason.chars().count() > 4_000 {
            return Err("reason exceeds 4000 characters".into());
        }
        if self.recorded_at_unix_ms <= 0 {
            return Err("recorded_at_unix_ms must be greater than zero".into());
        }
        Ok(())
    }
}

pub struct ApprovalDecisionJournal {
    directory: PathBuf,
    records: Mutex<Vec<RecordedToolApprovalDecision>>,
}

impl ApprovalDecisionJournal {
    pub fn open(directory: PathBuf) -> Result<Self, String> {
        let records = load_records(&directory)?;
        Ok(Self {
            directory,
            records: Mutex::new(records),
        })
    }

    pub fn snapshot(&self, owner_user_id: &str) -> Vec<RecordedToolApprovalDecision> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|record| record.owner_user_id == owner_user_id)
            .cloned()
            .collect()
    }

    pub fn record(&self, record: RecordedToolApprovalDecision) -> Result<(), String> {
        record.validate()?;
        let mut records = self
            .records
            .lock()
            .map_err(|_| "approval journal lock is unavailable".to_owned())?;
        if let Some(existing) = records.iter().find(|existing| {
            existing.matches(
                &record.owner_user_id,
                &record.invocation_id,
                record.invocation_version,
            )
        }) {
            return if existing.decision == record.decision
                && existing.decided_by == record.decided_by
                && existing.reason == record.reason
            {
                Ok(())
            } else {
                Err("a different decision is already recorded for this approval version".into())
            };
        }

        ensure_private_directory(&self.directory)?;
        while records.len() >= MAXIMUM_ENTRIES {
            let Some(oldest) = records.last().cloned() else {
                break;
            };
            remove_record_file(&self.directory, &oldest)?;
            records.pop();
        }
        write_record(&self.directory, &record)?;
        records.push(record);
        records.sort_by(|left, right| {
            (right.recorded_at_unix_ms, right.invocation_id.as_str())
                .cmp(&(left.recorded_at_unix_ms, left.invocation_id.as_str()))
        });
        Ok(())
    }

    pub fn remove(&self, record: &RecordedToolApprovalDecision) -> Result<(), String> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| "approval journal lock is unavailable".to_owned())?;
        remove_record_file(&self.directory, record)?;
        records.retain(|existing| {
            !existing.matches(
                &record.owner_user_id,
                &record.invocation_id,
                record.invocation_version,
            )
        });
        Ok(())
    }
}

fn load_records(directory: &Path) -> Result<Vec<RecordedToolApprovalDecision>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to read approval journal: {error}")),
    };
    let mut records = Vec::new();
    for entry in entries.flatten().take(MAXIMUM_ENTRIES * 2) {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAXIMUM_RECORD_BYTES {
            continue;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let Ok(file) = File::open(&path) else {
            continue;
        };
        if file
            .take(MAXIMUM_RECORD_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAXIMUM_RECORD_BYTES
        {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<RecordedToolApprovalDecision>(&bytes) else {
            continue;
        };
        if record.validate().is_ok() && record_path(directory, &record) == path {
            records.push(record);
        }
    }
    records.sort_by(|left, right| {
        (right.recorded_at_unix_ms, right.invocation_id.as_str())
            .cmp(&(left.recorded_at_unix_ms, left.invocation_id.as_str()))
    });
    records.truncate(MAXIMUM_ENTRIES);
    Ok(records)
}

fn write_record(directory: &Path, record: &RecordedToolApprovalDecision) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| format!("failed to encode approval journal: {error}"))?;
    if bytes.len() as u64 > MAXIMUM_RECORD_BYTES {
        return Err("approval journal record exceeds its size limit".into());
    }
    let destination = record_path(directory, record);
    if destination.exists() {
        return Err("approval journal identity already exists".into());
    }
    let temporary = directory.join(format!(".approval-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = private_file(&temporary)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("failed to persist approval journal: {error}"))?;
        fs::rename(&temporary, &destination)
            .map_err(|error| format!("failed to publish approval journal: {error}"))?;
        sync_directory(directory);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_record_file(
    directory: &Path,
    record: &RecordedToolApprovalDecision,
) -> Result<(), String> {
    match fs::remove_file(record_path(directory, record)) {
        Ok(()) => {
            sync_directory(directory);
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove approval journal record: {error}")),
    }
}

fn record_path(directory: &Path, record: &RecordedToolApprovalDecision) -> PathBuf {
    let mut digest = Sha256::new();
    let version = record.invocation_version.to_string();
    for value in [
        record.owner_user_id.as_str(),
        record.invocation_id.as_str(),
        version.as_str(),
    ] {
        digest.update(value.len().to_le_bytes());
        digest.update(value.as_bytes());
    }
    directory.join(format!("{:x}.json", digest.finalize()))
}

fn validate_identifier(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        Err(format!("{field} is invalid"))
    } else {
        Ok(())
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("failed to create approval journal directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to protect approval journal directory: {error}"))?;
    }
    Ok(())
}

#[cfg(unix)]
fn private_file(path: &Path) -> Result<File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("failed to create approval journal record: {error}"))
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create approval journal record: {error}"))
}

#[cfg(unix)]
fn sync_directory(path: &Path) {
    let _ = File::open(path).and_then(|directory| directory.sync_all());
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_exact_identity_and_decision_across_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let journal = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
        let record = sample_record(ToolApprovalDecision::Accept);
        journal.record(record.clone()).unwrap();
        drop(journal);

        let reopened = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
        assert_eq!(reopened.snapshot("owner-1"), vec![record.clone()]);
        reopened.remove(&record).unwrap();
        assert!(ApprovalDecisionJournal::open(directory.path().into())
            .unwrap()
            .snapshot("owner-1")
            .is_empty());
    }

    #[test]
    fn refuses_to_replace_a_recorded_decision_for_the_same_version() {
        let directory = tempfile::tempdir().unwrap();
        let journal = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
        journal
            .record(sample_record(ToolApprovalDecision::Accept))
            .unwrap();
        let error = journal
            .record(sample_record(ToolApprovalDecision::Decline))
            .unwrap_err();
        assert!(error.contains("different decision"));
        assert_eq!(
            journal.snapshot("owner-1")[0].decision,
            ToolApprovalDecision::Accept
        );
    }

    #[test]
    fn same_decision_is_idempotent_even_when_retry_time_changes() {
        let directory = tempfile::tempdir().unwrap();
        let journal = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
        let original = sample_record(ToolApprovalDecision::Accept);
        journal.record(original.clone()).unwrap();
        let mut retry = original.clone();
        retry.recorded_at_unix_ms += 100;
        journal.record(retry.clone()).unwrap();
        assert_eq!(journal.snapshot("owner-1"), vec![original]);
        journal.remove(&retry).unwrap();
        assert!(journal.snapshot("owner-1").is_empty());
    }

    #[test]
    fn ignores_corrupt_unbounded_and_misnamed_records() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("corrupt.json"), b"not-json").unwrap();
        fs::write(
            directory.path().join("oversized.json"),
            vec![b'x'; MAXIMUM_RECORD_BYTES as usize + 1],
        )
        .unwrap();
        fs::write(
            directory.path().join("wrong-name.json"),
            serde_json::to_vec(&sample_record(ToolApprovalDecision::Accept)).unwrap(),
        )
        .unwrap();
        assert!(ApprovalDecisionJournal::open(directory.path().into())
            .unwrap()
            .snapshot("owner-1")
            .is_empty());
    }

    fn sample_record(decision: ToolApprovalDecision) -> RecordedToolApprovalDecision {
        RecordedToolApprovalDecision {
            owner_user_id: "owner-1".into(),
            invocation_id: "invocation-1".into(),
            invocation_version: 2,
            decision,
            decided_by: "windows-react-human".into(),
            reason: "user decision".into(),
            recorded_at_unix_ms: 10,
        }
    }
}

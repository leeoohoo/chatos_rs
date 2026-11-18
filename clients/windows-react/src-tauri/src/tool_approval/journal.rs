use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::app_data_transaction::{create_owned_file, ensure_owned_directory};

use super::models::{ToolApprovalDecision, ToolApprovalMode};

const MAXIMUM_ENTRIES: usize = 256;
const MAXIMUM_RECORD_BYTES: u64 = 16 * 1_024;
const JOURNAL_UNAVAILABLE_ERROR: &str = "本机审批决定日志损坏或不可读，已停止审批恢复。";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordedToolApprovalDecision {
    pub owner_user_id: String,
    pub invocation_id: String,
    pub invocation_version: u64,
    #[serde(default)]
    pub mode: Option<ToolApprovalMode>,
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
    load_error: Option<String>,
    records: Mutex<Vec<RecordedToolApprovalDecision>>,
}

impl ApprovalDecisionJournal {
    pub fn open(directory: PathBuf) -> Result<Self, String> {
        let (records, load_error) = match load_records(&directory) {
            Ok(records) => (records, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Ok(Self {
            directory,
            load_error,
            records: Mutex::new(records),
        })
    }

    pub fn snapshot(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<RecordedToolApprovalDecision>, String> {
        self.require_available()?;
        Ok(self
            .records
            .lock()
            .map_err(|_| "approval journal lock is unavailable".to_owned())?
            .iter()
            .filter(|record| record.owner_user_id == owner_user_id)
            .cloned()
            .collect())
    }

    pub fn record(
        &self,
        record: RecordedToolApprovalDecision,
    ) -> Result<RecordedToolApprovalDecision, String> {
        self.require_available()?;
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
                Ok(existing.clone())
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
        records.push(record.clone());
        records.sort_by(|left, right| {
            (right.recorded_at_unix_ms, right.invocation_id.as_str())
                .cmp(&(left.recorded_at_unix_ms, left.invocation_id.as_str()))
        });
        Ok(record)
    }

    pub fn remove(&self, record: &RecordedToolApprovalDecision) -> Result<(), String> {
        self.require_available()?;
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

    fn require_available(&self) -> Result<(), String> {
        if self.load_error.is_some() {
            Err(JOURNAL_UNAVAILABLE_ERROR.to_owned())
        } else {
            Ok(())
        }
    }
}

fn load_records(directory: &Path) -> Result<Vec<RecordedToolApprovalDecision>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to read approval journal: {error}")),
    };
    let mut records = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read approval journal: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if records.len() >= MAXIMUM_ENTRIES {
            return Err("approval journal contains too many records".into());
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect approval journal: {error}"))?;
        if !file_type.is_file() || file_type.is_symlink() {
            return Err("approval journal contains a non-regular record".into());
        }
        let metadata = entry
            .metadata()
            .map_err(|error| format!("failed to inspect approval journal: {error}"))?;
        if !metadata.is_file() || metadata.len() > MAXIMUM_RECORD_BYTES {
            return Err("approval journal record exceeds its size limit".into());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let file = File::open(&path)
            .map_err(|error| format!("failed to read approval journal: {error}"))?;
        file.take(MAXIMUM_RECORD_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read approval journal: {error}"))?;
        if bytes.len() as u64 > MAXIMUM_RECORD_BYTES {
            return Err("approval journal record exceeds its size limit".into());
        }
        let record = serde_json::from_slice::<RecordedToolApprovalDecision>(&bytes)
            .map_err(|error| format!("failed to decode approval journal: {error}"))?;
        record.validate()?;
        if record_path(directory, &record) != path {
            return Err("approval journal record identity does not match its file name".into());
        }
        records.push(record);
    }
    records.sort_by(|left, right| {
        (right.recorded_at_unix_ms, right.invocation_id.as_str())
            .cmp(&(left.recorded_at_unix_ms, left.invocation_id.as_str()))
    });
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
    create_owned_file(directory, &destination, ".approval-", &bytes)
        .map_err(|error| format!("failed to persist approval journal: {error}"))?;
    sync_directory(directory);
    Ok(())
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
    ensure_owned_directory(path)
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
        let record = sample_record(ToolApprovalDecision::AcceptForSession);
        assert_eq!(journal.record(record.clone()).unwrap(), record);
        drop(journal);

        let reopened = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
        assert_eq!(reopened.snapshot("owner-1").unwrap(), vec![record.clone()]);
        reopened.remove(&record).unwrap();
        assert!(ApprovalDecisionJournal::open(directory.path().into())
            .unwrap()
            .snapshot("owner-1")
            .unwrap()
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
            journal.snapshot("owner-1").unwrap()[0].decision,
            ToolApprovalDecision::Accept
        );
    }

    #[test]
    fn same_decision_preserves_original_mode_and_time_across_retries() {
        let directory = tempfile::tempdir().unwrap();
        let journal = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
        let original = sample_record(ToolApprovalDecision::Accept);
        journal.record(original.clone()).unwrap();
        let mut retry = original.clone();
        retry.recorded_at_unix_ms += 100;
        retry.mode = Some(ToolApprovalMode::FullControl);
        assert_eq!(journal.record(retry.clone()).unwrap(), original);
        assert_eq!(journal.snapshot("owner-1").unwrap(), vec![original]);
        journal.remove(&retry).unwrap();
        assert!(journal.snapshot("owner-1").unwrap().is_empty());
    }

    #[test]
    fn loads_journal_records_written_before_mode_was_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let mut record = sample_record(ToolApprovalDecision::Accept);
        record.mode = None;
        let mut value = serde_json::to_value(&record).unwrap();
        value.as_object_mut().unwrap().remove("mode");
        fs::write(
            record_path(directory.path(), &record),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();

        let journal = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
        assert_eq!(journal.snapshot("owner-1").unwrap(), vec![record]);
    }

    #[test]
    fn corrupt_unbounded_and_misnamed_records_fail_closed_without_overwrite() {
        let mut invalid = sample_record(ToolApprovalDecision::Accept);
        invalid.invocation_version = 0;
        let invalid_name = record_path(Path::new(""), &invalid)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for (name, bytes) in [
            ("corrupt.json".to_owned(), b"not-json".to_vec()),
            (
                "oversized.json".to_owned(),
                vec![b'x'; MAXIMUM_RECORD_BYTES as usize + 1],
            ),
            (
                "wrong-name.json".to_owned(),
                serde_json::to_vec(&sample_record(ToolApprovalDecision::Accept)).unwrap(),
            ),
            (invalid_name, serde_json::to_vec(&invalid).unwrap()),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join(name);
            fs::write(&path, &bytes).unwrap();
            let journal = ApprovalDecisionJournal::open(directory.path().into()).unwrap();
            assert_eq!(
                journal.snapshot("owner-1").unwrap_err(),
                JOURNAL_UNAVAILABLE_ERROR
            );
            assert_eq!(
                journal
                    .record(sample_record(ToolApprovalDecision::Decline))
                    .unwrap_err(),
                JOURNAL_UNAVAILABLE_ERROR
            );
            assert_eq!(
                journal
                    .remove(&sample_record(ToolApprovalDecision::Accept))
                    .unwrap_err(),
                JOURNAL_UNAVAILABLE_ERROR
            );
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }

    fn sample_record(decision: ToolApprovalDecision) -> RecordedToolApprovalDecision {
        RecordedToolApprovalDecision {
            owner_user_id: "owner-1".into(),
            invocation_id: "invocation-1".into(),
            invocation_version: 2,
            mode: Some(ToolApprovalMode::RequestApproval),
            decision,
            decided_by: "windows-react-human".into(),
            reason: "user decision".into(),
            recorded_at_unix_ms: 10,
        }
    }
}

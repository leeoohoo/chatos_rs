use super::{
    journal_error, LocalAgentToolOutcome, MAXIMUM_JOURNAL_BYTES, MAXIMUM_JOURNAL_TEMPORARY_ENTRIES,
    STALE_JOURNAL_TEMPORARY_AGE,
};
use crate::{
    app_data_transaction::ensure_owned_directory, app_owned_export::try_remove_stale_owned_file,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
pub(super) struct InvocationJournal {
    pub(super) root: PathBuf,
}

pub(super) enum JournalBegin {
    Execute,
    Replay(LocalAgentToolOutcome),
    Uncertain,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct JournalRecord {
    signature: String,
    status: JournalStatus,
    outcome: Option<LocalAgentToolOutcome>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JournalStatus {
    Running,
    Completed,
    Uncertain,
}

impl InvocationJournal {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(super) async fn begin(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
        signature: &str,
    ) -> Result<JournalBegin, String> {
        let base = self.path(owner, run_id, invocation_id).await?;
        if let Some(record) = read_journal_record(&journal_stage_path(&base, "result")).await? {
            if record.signature != signature {
                return Err(
                    "Plugin invocation identity was reused with different arguments.".into(),
                );
            }
            return Ok(match (record.status, record.outcome) {
                (JournalStatus::Completed, Some(outcome)) => JournalBegin::Replay(outcome),
                _ => JournalBegin::Uncertain,
            });
        }
        for stage in ["uncertain", "intent"] {
            if let Some(record) = read_journal_record(&journal_stage_path(&base, stage)).await? {
                if record.signature != signature {
                    return Err(
                        "Plugin invocation identity was reused with different arguments.".into(),
                    );
                }
                return Ok(JournalBegin::Uncertain);
            }
        }
        write_journal_record(
            &journal_stage_path(&base, "intent"),
            &JournalRecord {
                signature: signature.to_owned(),
                status: JournalStatus::Running,
                outcome: None,
            },
        )
        .await?;
        Ok(JournalBegin::Execute)
    }

    pub(super) async fn complete(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
        signature: &str,
        outcome: LocalAgentToolOutcome,
    ) -> Result<(), String> {
        let base = self.path(owner, run_id, invocation_id).await?;
        write_journal_record(
            &journal_stage_path(&base, "result"),
            &JournalRecord {
                signature: signature.to_owned(),
                status: JournalStatus::Completed,
                outcome: Some(outcome),
            },
        )
        .await
    }

    pub(super) async fn mark_uncertain(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
        signature: &str,
    ) -> Result<(), String> {
        let base = self.path(owner, run_id, invocation_id).await?;
        write_journal_record(
            &journal_stage_path(&base, "uncertain"),
            &JournalRecord {
                signature: signature.to_owned(),
                status: JournalStatus::Uncertain,
                outcome: None,
            },
        )
        .await
    }

    pub(super) async fn path(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
    ) -> Result<PathBuf, String> {
        ensure_private_journal_root(&self.root).await?;
        let digest = Sha256::digest(format!("{owner}\0{run_id}\0{invocation_id}").as_bytes());
        Ok(self.root.join(format!("{digest:x}")))
    }
}

async fn ensure_private_journal_root(root: &Path) -> Result<(), String> {
    ensure_owned_directory(root).map_err(|_| journal_error())?;
    match tokio::fs::symlink_metadata(root).await {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(journal_error()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(journal_error()),
        Err(_) => return Err(journal_error()),
    }
    let metadata = tokio::fs::symlink_metadata(root)
        .await
        .map_err(|_| journal_error())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(journal_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|_| journal_error())?;
    }
    let cleanup_root = root.to_owned();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let cutoff = SystemTime::now()
            .checked_sub(STALE_JOURNAL_TEMPORARY_AGE)
            .unwrap_or(UNIX_EPOCH);
        cleanup_stale_journal_temporaries_before(
            &cleanup_root,
            cutoff,
            MAXIMUM_JOURNAL_TEMPORARY_ENTRIES,
        )
    })
    .await;
    Ok(())
}

pub(super) fn cleanup_stale_journal_temporaries_before(
    root: &Path,
    cutoff: SystemTime,
    maximum_entries: usize,
) -> usize {
    if maximum_entries == 0 {
        return 0;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0usize;
    for (visited, entry) in entries.enumerate() {
        if visited >= maximum_entries {
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if canonical_journal_temporary_name(&name)
            && try_remove_stale_owned_file(&entry.path(), cutoff)
        {
            removed = removed.saturating_add(1);
        }
    }
    removed
}

fn canonical_journal_temporary_name(name: &str) -> bool {
    name.strip_prefix('.')
        .and_then(|value| value.strip_suffix(".tmp"))
        .and_then(|value| {
            uuid::Uuid::parse_str(value)
                .ok()
                .filter(|uuid| uuid.to_string() == value)
        })
        .is_some()
}

pub(super) fn journal_stage_path(base: &Path, stage: &str) -> PathBuf {
    base.with_extension(format!("{stage}.json"))
}

async fn read_journal_record(path: &Path) -> Result<Option<JournalRecord>, String> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(journal_error()),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAXIMUM_JOURNAL_BYTES
    {
        return Err(journal_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(MAXIMUM_JOURNAL_BYTES) as usize + 1);
    tokio::fs::File::open(path)
        .await
        .map_err(|_| journal_error())?
        .take(MAXIMUM_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| journal_error())?;
    if bytes.len() as u64 > MAXIMUM_JOURNAL_BYTES {
        return Err(journal_error());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| journal_error())
}

async fn write_journal_record(path: &Path, record: &JournalRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec(record).map_err(|_| journal_error())?;
    if bytes.len() as u64 > MAXIMUM_JOURNAL_BYTES {
        return Err(journal_error());
    }
    let parent = path.parent().ok_or_else(journal_error)?;
    ensure_private_journal_root(parent).await?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .await
        .map_err(|_| journal_error())?;
    if file.write_all(&bytes).await.is_err() || file.sync_all().await.is_err() {
        drop(file);
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(journal_error());
    }
    drop(file);
    match tokio::fs::hard_link(&temporary, path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_journal_record(path).await?.as_ref() != Some(record) {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(journal_error());
            }
        }
        Err(_) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(journal_error());
        }
    }
    tokio::fs::remove_file(&temporary)
        .await
        .map_err(|_| journal_error())?;
    Ok(())
}

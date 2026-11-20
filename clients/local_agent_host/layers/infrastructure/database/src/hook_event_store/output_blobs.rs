#[derive(Debug)]
struct PreparedOutputBlob {
    record: LocalHookOutputBlobRecord,
    path: std::path::PathBuf,
    created: bool,
}

async fn prepare_output_blobs(
    root: std::path::PathBuf,
    invocation_id: String,
    attempt: u32,
    completion: LocalHookInvocationCompletion,
    now_unix_ms: i64,
) -> Result<Vec<PreparedOutputBlob>, ClientStorageError> {
    tokio::task::spawn_blocking(move || {
        let mut prepared = Vec::new();
        for (stream, spill) in [
            ("stdout", completion.stdout_spill.as_ref()),
            ("stderr", completion.stderr_spill.as_ref()),
        ] {
            let Some(spill) = spill else { continue };
            let expires_at_unix_ms =
                now_unix_ms
                    .checked_add(OUTPUT_RETENTION_MS)
                    .ok_or_else(|| {
                        ClientStorageError::InvalidState("Hook output expiry overflow".to_string())
                    })?;
            let blob_id = hook_output_blob_id(&invocation_id, attempt, stream, spill.as_bytes());
            let storage_ref = format!("{blob_id}.hook-output");
            let path = safe_output_path(&root, &storage_ref)?;
            let created = write_output_file(&path, spill.as_bytes())?;
            prepared.push(PreparedOutputBlob {
                record: LocalHookOutputBlobRecord {
                    blob_id,
                    invocation_id: invocation_id.clone(),
                    stream: stream.to_string(),
                    sha256: format!("{:x}", Sha256::digest(spill.as_bytes())),
                    byte_count: u64::try_from(spill.byte_count()).map_err(|_| {
                        ClientStorageError::InvalidState(
                            "Hook output blob size overflow".to_string(),
                        )
                    })?,
                    storage_ref,
                    expires_at_unix_ms,
                    created_at_unix_ms: now_unix_ms,
                },
                path,
                created,
            });
        }
        Ok(prepared)
    })
    .await
    .map_err(ClientStorageError::database)?
}

fn hook_output_blob_id(invocation_id: &str, attempt: u32, stream: &str, bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(invocation_id.as_bytes());
    digest.update([0]);
    digest.update(attempt.to_be_bytes());
    digest.update([0]);
    digest.update(stream.as_bytes());
    digest.update([0]);
    digest.update(Sha256::digest(bytes));
    format!("{:x}", digest.finalize())
}

fn safe_output_path(
    root: &Path,
    storage_ref: &str,
) -> Result<std::path::PathBuf, ClientStorageError> {
    let Some(blob_id) = storage_ref.strip_suffix(".hook-output") else {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook output storage reference".to_string(),
        ));
    };
    if !is_sha256(blob_id) || storage_ref.contains('/') || storage_ref.contains('\\') {
        return Err(ClientStorageError::InvalidState(
            "invalid Hook output storage reference".to_string(),
        ));
    }
    Ok(root.join(storage_ref))
}

fn write_output_file(path: &Path, bytes: &[u8]) -> Result<bool, ClientStorageError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(ClientStorageError::database)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path).map_err(ClientStorageError::database)?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(ClientStorageError::InvalidState(
                    "Hook output path is not a regular file".to_string(),
                ));
            }
            let existing = std::fs::read(path).map_err(ClientStorageError::database)?;
            if existing != bytes {
                return Err(ClientStorageError::Conflict(
                    "Hook output blob identity contains different bytes".to_string(),
                ));
            }
            Ok(false)
        }
        Err(error) => Err(ClientStorageError::database(error)),
    }
}

async fn cleanup_created_blobs(prepared: &[PreparedOutputBlob]) {
    for blob in prepared.iter().filter(|blob| blob.created) {
        let _ = tokio::fs::remove_file(&blob.path).await;
    }
}

async fn cleanup_expired_blobs(root: &Path, storage_refs: &[String]) {
    for storage_ref in storage_refs {
        let Ok(path) = safe_output_path(root, storage_ref) else {
            continue;
        };
        let _ = tokio::fs::remove_file(path).await;
    }
}

async fn cleanup_unreferenced_output_files(
    root: std::path::PathBuf,
    retained: Vec<String>,
) -> Result<(), ClientStorageError> {
    tokio::task::spawn_blocking(move || {
        let retained = retained
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        for entry in std::fs::read_dir(&root).map_err(ClientStorageError::database)? {
            let entry = entry.map_err(ClientStorageError::database)?;
            let Some(name) = entry.file_name().to_str().map(ToString::to_string) else {
                continue;
            };
            if retained.contains(&name) || safe_output_path(&root, &name).is_err() {
                continue;
            }
            let metadata =
                std::fs::symlink_metadata(entry.path()).map_err(ClientStorageError::database)?;
            if metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    })
    .await
    .map_err(ClientStorageError::database)?
}

fn decode_output_blob(row: SqliteRow) -> Result<LocalHookOutputBlobRecord, ClientStorageError> {
    Ok(LocalHookOutputBlobRecord {
        blob_id: row.try_get("blob_id").db()?,
        invocation_id: row.try_get("invocation_id").db()?,
        stream: row.try_get("stream").db()?,
        sha256: row.try_get("sha256").db()?,
        byte_count: to_u64(row.try_get("byte_count").db()?, "Hook output blob size")?,
        storage_ref: row.try_get("storage_ref").db()?,
        expires_at_unix_ms: row.try_get("expires_at_unix_ms").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
    })
}

fn validate_output_identifier(name: &str, value: &str) -> Result<(), ClientStorageError> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ClientStorageError::InvalidState(format!("invalid {name}")));
    }
    Ok(())
}

fn to_i64(value: u64, name: &str) -> Result<i64, ClientStorageError> {
    i64::try_from(value).map_err(|_| ClientStorageError::InvalidState(format!("{name} overflow")))
}

fn to_u64(value: i64, name: &str) -> Result<u64, ClientStorageError> {
    u64::try_from(value).map_err(|_| ClientStorageError::InvalidState(format!("invalid {name}")))
}

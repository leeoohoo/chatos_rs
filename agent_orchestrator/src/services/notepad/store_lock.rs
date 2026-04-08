use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::fs;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;

use super::store_normalize::now_iso;

const LOCK_TIMEOUT_MS: u64 = 10_000;
const LOCK_STALE_MS: u64 = 30_000;
const LOCK_POLL_MS: u64 = 25;

pub struct FileLockGuard {
    path: PathBuf,
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn acquire_file_lock(path: &Path) -> Result<FileLockGuard, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }

    let start = std::time::Instant::now();
    loop {
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .await
        {
            Ok(mut file) => {
                let payload = format!(
                    "{{\"pid\":{},\"started_at\":\"{}\"}}",
                    std::process::id(),
                    now_iso()
                );
                let _ = file.write_all(payload.as_bytes()).await;
                return Ok(FileLockGuard {
                    path: path.to_path_buf(),
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if let Ok(meta) = fs::metadata(path).await {
                    if let Ok(modified_at) = meta.modified() {
                        if let Ok(elapsed) = modified_at.elapsed() {
                            if elapsed > Duration::from_millis(LOCK_STALE_MS) {
                                let _ = fs::remove_file(path).await;
                                continue;
                            }
                        }
                    }
                }
                if start.elapsed() > Duration::from_millis(LOCK_TIMEOUT_MS) {
                    return Err(format!(
                        "Timed out waiting for lock ({})",
                        path.file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or("notes.lock")
                    ));
                }
                sleep(Duration::from_millis(LOCK_POLL_MS)).await;
            }
            Err(err) => return Err(err.to_string()),
        }
    }
}

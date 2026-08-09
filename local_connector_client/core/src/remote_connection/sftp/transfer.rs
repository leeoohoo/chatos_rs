// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use uuid::Uuid;

const MAX_RETAINED_TRANSFERS: usize = 256;

#[derive(Clone, Debug, Default)]
pub(crate) struct RemoteSftpManager {
    transfers: Arc<Mutex<BTreeMap<String, TransferRecord>>>,
}

#[derive(Clone, Debug)]
struct TransferRecord {
    status: TransferStatus,
    cancel: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TransferStatus {
    pub(super) id: String,
    connection_id: String,
    direction: String,
    state: String,
    total_bytes: Option<u64>,
    transferred_bytes: u64,
    percent: Option<f64>,
    current_path: Option<String>,
    message: Option<String>,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RemoteSftpManager {
    pub(super) fn create(
        &self,
        connection_id: String,
        direction: String,
        current_path: Option<String>,
    ) -> TransferStatus {
        let now = chrono::Utc::now().to_rfc3339();
        let status = TransferStatus {
            id: Uuid::new_v4().to_string(),
            connection_id,
            direction,
            state: "pending".to_string(),
            total_bytes: None,
            transferred_bytes: 0,
            percent: None,
            current_path,
            message: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
        };
        let mut transfers = self.records();
        prune_completed(&mut transfers);
        transfers.insert(
            status.id.clone(),
            TransferRecord {
                status: status.clone(),
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        status
    }

    pub(super) fn status(
        &self,
        connection_id: &str,
        transfer_id: &str,
    ) -> Result<TransferStatus, String> {
        let transfers = self.records();
        let record = transfers
            .get(transfer_id)
            .ok_or_else(|| "transfer not found".to_string())?;
        ensure_connection(record, connection_id)?;
        Ok(record.status.clone())
    }

    pub(super) fn cancel(
        &self,
        connection_id: &str,
        transfer_id: &str,
    ) -> Result<TransferStatus, String> {
        let mut transfers = self.records();
        let record = transfers
            .get_mut(transfer_id)
            .ok_or_else(|| "transfer not found".to_string())?;
        ensure_connection(record, connection_id)?;
        if is_terminal_state(record.status.state.as_str()) {
            return Err("transfer is not active".to_string());
        }
        record.cancel.store(true, Ordering::SeqCst);
        record.status.state = "cancelling".to_string();
        record.status.message = Some("正在取消传输...".to_string());
        record.status.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(record.status.clone())
    }

    pub(super) fn set_running(&self, transfer_id: &str) {
        self.update(transfer_id, |status| {
            if status.state == "pending" {
                status.state = "running".to_string();
            }
        });
    }

    pub(super) fn set_total(&self, transfer_id: &str, total_bytes: u64, path: String) {
        self.update(transfer_id, |status| {
            status.total_bytes = Some(total_bytes);
            status.current_path = Some(path);
            status.percent = Some(if total_bytes == 0 { 100.0 } else { 0.0 });
        });
    }

    pub(super) fn finish(&self, transfer_id: &str, message: String) {
        self.update(transfer_id, |status| {
            status.state = "success".to_string();
            status.transferred_bytes = status.total_bytes.unwrap_or(status.transferred_bytes);
            status.percent = Some(100.0);
            status.message = Some(message);
            status.error = None;
        });
    }

    pub(super) fn fail(&self, transfer_id: &str, error: String) {
        self.update(transfer_id, |status| {
            status.state = "error".to_string();
            status.error = Some(error);
        });
    }

    pub(super) fn mark_cancelled(&self, transfer_id: &str) {
        self.update(transfer_id, |status| {
            status.state = "cancelled".to_string();
            status.message = Some("传输已取消".to_string());
        });
    }

    pub(super) fn is_cancelled(&self, transfer_id: &str) -> bool {
        self.records()
            .get(transfer_id)
            .is_some_and(|record| record.cancel.load(Ordering::SeqCst))
    }

    fn add_progress(&self, transfer_id: &str, bytes: u64, path: String) {
        self.update(transfer_id, |status| {
            status.transferred_bytes = status.transferred_bytes.saturating_add(bytes);
            status.current_path = Some(path);
            status.percent = status.total_bytes.map(|total| {
                if total == 0 {
                    100.0
                } else {
                    (status.transferred_bytes as f64 * 100.0 / total as f64).clamp(0.0, 100.0)
                }
            });
        });
    }

    fn update(&self, transfer_id: &str, apply: impl FnOnce(&mut TransferStatus)) {
        if let Some(record) = self.records().get_mut(transfer_id) {
            apply(&mut record.status);
            record.status.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }

    fn records(&self) -> MutexGuard<'_, BTreeMap<String, TransferRecord>> {
        self.transfers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Clone)]
pub(super) struct TransferProgress {
    transfer_id: String,
    manager: RemoteSftpManager,
}

impl TransferProgress {
    pub(super) fn new(transfer_id: String, manager: RemoteSftpManager) -> Self {
        Self {
            transfer_id,
            manager,
        }
    }

    pub(super) fn check(&self) -> Result<(), String> {
        if self.manager.is_cancelled(self.transfer_id.as_str()) {
            Err("transfer cancelled".to_string())
        } else {
            Ok(())
        }
    }

    pub(super) fn set_total(&self, total_bytes: u64, path: String) {
        self.manager
            .set_total(self.transfer_id.as_str(), total_bytes, path);
    }

    pub(super) fn add(&self, bytes: u64, path: String) {
        self.manager
            .add_progress(self.transfer_id.as_str(), bytes, path);
    }
}

fn ensure_connection(record: &TransferRecord, connection_id: &str) -> Result<(), String> {
    if record.status.connection_id == connection_id {
        Ok(())
    } else {
        Err("transfer not found".to_string())
    }
}

fn is_terminal_state(state: &str) -> bool {
    matches!(state, "success" | "error" | "cancelled")
}

fn prune_completed(transfers: &mut BTreeMap<String, TransferRecord>) {
    while transfers.len() >= MAX_RETAINED_TRANSFERS {
        let oldest_completed = transfers
            .iter()
            .filter(|(_, record)| is_terminal_state(record.status.state.as_str()))
            .min_by_key(|(_, record)| record.status.updated_at.as_str())
            .map(|(id, _)| id.clone());
        let Some(id) = oldest_completed else {
            break;
        };
        transfers.remove(id.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_status_is_scoped_to_connection() {
        let manager = RemoteSftpManager::default();
        let status = manager.create(
            "connection-a".to_string(),
            "upload".to_string(),
            Some("/tmp/file".to_string()),
        );

        assert!(manager.status("connection-a", status.id.as_str()).is_ok());
        assert_eq!(
            manager
                .status("connection-b", status.id.as_str())
                .unwrap_err(),
            "transfer not found"
        );
    }

    #[test]
    fn directory_progress_accumulates_across_files() {
        let manager = RemoteSftpManager::default();
        let status = manager.create(
            "connection-a".to_string(),
            "download".to_string(),
            Some("/remote".to_string()),
        );
        let progress = TransferProgress::new(status.id.clone(), manager.clone());
        progress.set_total(100, "/remote".to_string());
        progress.add(25, "/remote/a".to_string());
        progress.add(50, "/remote/b".to_string());

        let status = manager.status("connection-a", status.id.as_str()).unwrap();
        assert_eq!(status.transferred_bytes, 75);
        assert_eq!(status.percent, Some(75.0));
    }

    #[test]
    fn cancellation_cannot_be_overwritten_by_worker_start() {
        let manager = RemoteSftpManager::default();
        let status = manager.create(
            "connection-a".to_string(),
            "upload".to_string(),
            Some("/remote".to_string()),
        );
        manager.cancel("connection-a", status.id.as_str()).unwrap();

        manager.set_running(status.id.as_str());

        let status = manager.status("connection-a", status.id.as_str()).unwrap();
        assert_eq!(status.state, "cancelling");
    }
}

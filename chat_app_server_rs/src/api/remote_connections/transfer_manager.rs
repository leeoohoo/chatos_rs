use dashmap::DashMap;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use super::SftpTransferStatus;
use crate::services::realtime::{
    publish_remote_sftp_transfer_updated, RemoteSftpTransferRealtimePayload,
};

pub(super) struct SftpTransferManager {
    transfers: DashMap<String, SftpTransferStatus>,
    cancel_flags: DashMap<String, bool>,
    publish_gates: DashMap<String, TransferPublishGate>,
}

#[derive(Clone, Debug)]
struct TransferPublishGate {
    last_state: String,
    last_percent_bucket: Option<i64>,
    last_published_at: Instant,
}

impl SftpTransferManager {
    fn new() -> Self {
        Self {
            transfers: DashMap::new(),
            cancel_flags: DashMap::new(),
            publish_gates: DashMap::new(),
        }
    }

    pub(super) fn create(
        &self,
        connection_id: &str,
        user_id: &str,
        direction: &str,
        total_bytes: Option<u64>,
        current_path: Option<String>,
    ) -> SftpTransferStatus {
        let id = Uuid::new_v4().to_string();
        let now = crate::core::time::now_rfc3339();
        let status = SftpTransferStatus {
            id: id.clone(),
            connection_id: connection_id.to_string(),
            user_id: user_id.to_string(),
            direction: direction.to_string(),
            state: "pending".to_string(),
            total_bytes,
            transferred_bytes: 0,
            percent: total_bytes.and_then(|total| if total == 0 { Some(100.0) } else { Some(0.0) }),
            current_path,
            message: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.transfers.insert(id, status.clone());
        self.cancel_flags.insert(status.id.clone(), false);
        self.publish_status(&status, true);
        status
    }

    pub(super) fn get_for_connection(
        &self,
        transfer_id: &str,
        connection_id: &str,
    ) -> Option<SftpTransferStatus> {
        self.transfers.get(transfer_id).and_then(|entry| {
            if entry.connection_id == connection_id {
                Some(entry.clone())
            } else {
                None
            }
        })
    }

    pub(super) fn set_running(&self, transfer_id: &str) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            if self.is_cancel_requested(transfer_id) {
                entry.state = "cancelling".to_string();
                entry.message = Some("正在取消传输...".to_string());
                entry.updated_at = crate::core::time::now_rfc3339();
                let snapshot = entry.clone();
                drop(entry);
                self.publish_status(&snapshot, true);
                return;
            }
            entry.state = "running".to_string();
            entry.updated_at = crate::core::time::now_rfc3339();
            entry.error = None;
            entry.message = None;
            let snapshot = entry.clone();
            drop(entry);
            self.publish_status(&snapshot, true);
        }
    }

    pub(super) fn set_progress(
        &self,
        transfer_id: &str,
        transferred_bytes: u64,
        total_bytes: Option<u64>,
        current_path: Option<String>,
    ) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.transferred_bytes = transferred_bytes;
            if total_bytes.is_some() {
                entry.total_bytes = total_bytes;
            }
            entry.current_path = current_path;
            entry.percent = entry.total_bytes.and_then(|total| {
                if total == 0 {
                    Some(100.0)
                } else {
                    Some(
                        ((entry.transferred_bytes as f64 * 100.0) / total as f64).clamp(0.0, 100.0),
                    )
                }
            });
            entry.updated_at = crate::core::time::now_rfc3339();
            let snapshot = entry.clone();
            drop(entry);
            self.publish_status(&snapshot, false);
        }
    }

    pub(super) fn set_done(&self, transfer_id: &str, message: String) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.state = "success".to_string();
            if let Some(total) = entry.total_bytes {
                entry.transferred_bytes = total;
                entry.percent = Some(100.0);
            } else if entry.transferred_bytes > 0 {
                entry.percent = Some(100.0);
            }
            entry.message = Some(message);
            entry.error = None;
            entry.updated_at = crate::core::time::now_rfc3339();
            let snapshot = entry.clone();
            drop(entry);
            self.publish_status(&snapshot, true);
        }
        self.cancel_flags.remove(transfer_id);
    }

    pub(super) fn set_error(&self, transfer_id: &str, error: String) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.state = "error".to_string();
            entry.error = Some(error);
            entry.message = None;
            entry.updated_at = crate::core::time::now_rfc3339();
            let snapshot = entry.clone();
            drop(entry);
            self.publish_status(&snapshot, true);
        }
        self.cancel_flags.remove(transfer_id);
    }

    pub(super) fn set_cancelled(&self, transfer_id: &str) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.state = "cancelled".to_string();
            entry.message = Some("传输已取消".to_string());
            entry.error = None;
            entry.updated_at = crate::core::time::now_rfc3339();
            let snapshot = entry.clone();
            drop(entry);
            self.publish_status(&snapshot, true);
        }
        self.cancel_flags.remove(transfer_id);
    }

    pub(super) fn request_cancel_for_connection(
        &self,
        transfer_id: &str,
        connection_id: &str,
    ) -> bool {
        let Some(mut entry) = self.transfers.get_mut(transfer_id) else {
            return false;
        };
        if entry.connection_id != connection_id {
            return false;
        }
        match entry.state.as_str() {
            "success" | "error" | "cancelled" => false,
            _ => {
                entry.state = "cancelling".to_string();
                entry.message = Some("正在取消传输...".to_string());
                entry.updated_at = crate::core::time::now_rfc3339();
                self.cancel_flags.insert(transfer_id.to_string(), true);
                let snapshot = entry.clone();
                drop(entry);
                self.publish_status(&snapshot, true);
                true
            }
        }
    }

    pub(super) fn is_cancel_requested(&self, transfer_id: &str) -> bool {
        self.cancel_flags
            .get(transfer_id)
            .map(|v| *v)
            .unwrap_or(false)
    }

    fn publish_status(&self, status: &SftpTransferStatus, force: bool) {
        if status.user_id.trim().is_empty() {
            return;
        }
        let now = Instant::now();
        let next_bucket = percent_bucket(status.percent);
        let should_publish = match self.publish_gates.get(status.id.as_str()) {
            Some(gate) => {
                force
                    || gate.last_state != status.state
                    || gate.last_percent_bucket != next_bucket
                    || now.duration_since(gate.last_published_at).as_millis() >= 1000
            }
            None => true,
        };
        if !should_publish {
            return;
        }

        self.publish_gates.insert(
            status.id.clone(),
            TransferPublishGate {
                last_state: status.state.clone(),
                last_percent_bucket: next_bucket,
                last_published_at: now,
            },
        );

        publish_remote_sftp_transfer_updated(
            status.user_id.as_str(),
            RemoteSftpTransferRealtimePayload {
                id: status.id.clone(),
                connection_id: status.connection_id.clone(),
                direction: status.direction.clone(),
                state: status.state.clone(),
                total_bytes: status.total_bytes,
                transferred_bytes: status.transferred_bytes,
                percent: status.percent,
                current_path: status.current_path.clone(),
                message: status.message.clone(),
                error: status.error.clone(),
                created_at: status.created_at.clone(),
                updated_at: status.updated_at.clone(),
            },
        );
    }
}

static SFTP_TRANSFER_MANAGER: OnceCell<Arc<SftpTransferManager>> = OnceCell::new();

pub(super) fn get_sftp_transfer_manager() -> Arc<SftpTransferManager> {
    SFTP_TRANSFER_MANAGER
        .get_or_init(|| Arc::new(SftpTransferManager::new()))
        .clone()
}

fn percent_bucket(value: Option<f64>) -> Option<i64> {
    value.map(|current| (current / 2.0).floor() as i64)
}

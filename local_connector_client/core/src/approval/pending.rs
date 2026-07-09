// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::fingerprint::normalized_command;
use super::types::{CommandApprovalRequest, PendingApprovalItem};

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug)]
pub(crate) struct PendingApprovalDecision {
    pub(crate) approved: bool,
    pub(crate) remember_allow: bool,
    pub(crate) reason: Option<String>,
}

struct PendingApprovalState {
    item: PendingApprovalItem,
    tx: Option<oneshot::Sender<PendingApprovalDecision>>,
}

fn pending_item_for_request(
    id: String,
    request: &CommandApprovalRequest,
    risk: String,
    reason: Option<String>,
) -> PendingApprovalItem {
    PendingApprovalItem {
        id,
        request_id: request.request_id.clone(),
        project_key: request.project_key.clone(),
        command: normalized_command(request.command.as_str(), request.args.as_slice()),
        cwd: request.cwd.clone(),
        source: request.source.clone(),
        risk,
        reason,
        created_at: local_now_rfc3339(),
    }
}

fn pending_store() -> &'static Mutex<BTreeMap<String, PendingApprovalState>> {
    static STORE: OnceLock<Mutex<BTreeMap<String, PendingApprovalState>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn in_progress_store() -> &'static Mutex<BTreeMap<String, PendingApprovalItem>> {
    static STORE: OnceLock<Mutex<BTreeMap<String, PendingApprovalItem>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) async fn request_pending_approval(
    request: &CommandApprovalRequest,
    risk: String,
    reason: Option<String>,
) -> PendingApprovalDecision {
    let id = format!("approval-{}", Uuid::new_v4());
    let item = pending_item_for_request(id.clone(), request, risk, reason);
    let (tx, rx) = oneshot::channel();
    {
        let mut pending = pending_store().lock().await;
        pending.insert(id.clone(), PendingApprovalState { item, tx: Some(tx) });
    }

    let result = tokio::time::timeout(APPROVAL_TIMEOUT, rx).await;
    {
        let mut pending = pending_store().lock().await;
        pending.remove(id.as_str());
    }
    match result {
        Ok(Ok(decision)) => decision,
        Ok(Err(_)) => PendingApprovalDecision {
            approved: false,
            remember_allow: false,
            reason: Some("approval request was cancelled".to_string()),
        },
        Err(_) => PendingApprovalDecision {
            approved: false,
            remember_allow: false,
            reason: Some("approval request timed out".to_string()),
        },
    }
}

pub(crate) async fn start_in_progress_approval(
    request: &CommandApprovalRequest,
    risk: String,
    reason: Option<String>,
) -> String {
    let id = format!("approval-running-{}", Uuid::new_v4());
    let item = pending_item_for_request(id.clone(), request, risk, reason);
    in_progress_store().lock().await.insert(id.clone(), item);
    id
}

pub(crate) async fn finish_in_progress_approval(id: &str) {
    in_progress_store().lock().await.remove(id);
}

pub(crate) async fn list_in_progress_approvals() -> Vec<PendingApprovalItem> {
    in_progress_store().lock().await.values().cloned().collect()
}

pub(crate) async fn list_pending_approvals() -> Vec<PendingApprovalItem> {
    pending_store()
        .lock()
        .await
        .values()
        .map(|entry| entry.item.clone())
        .collect()
}

pub(crate) async fn approve_pending_approval(id: &str, remember_allow: bool) -> bool {
    resolve_pending_approval(
        id,
        PendingApprovalDecision {
            approved: true,
            remember_allow,
            reason: None,
        },
    )
    .await
}

pub(crate) async fn deny_pending_approval(id: &str, reason: Option<String>) -> bool {
    resolve_pending_approval(
        id,
        PendingApprovalDecision {
            approved: false,
            remember_allow: false,
            reason,
        },
    )
    .await
}

async fn resolve_pending_approval(id: &str, decision: PendingApprovalDecision) -> bool {
    let mut pending = pending_store().lock().await;
    let Some(entry) = pending.get_mut(id) else {
        return false;
    };
    let Some(tx) = entry.tx.take() else {
        return false;
    };
    tx.send(decision).is_ok()
}

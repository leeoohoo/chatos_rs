// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

use chatos_sandbox_contract::{
    CommandExecutionApprovalDecision, GrantedPermissionProfile, PermissionGrantScope,
    SimpleCommandExecutionApprovalDecision,
};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::fingerprint::normalized_command;
use super::types::{CommandApprovalRequest, PendingApprovalItem};

#[cfg(not(test))]
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
#[cfg(test)]
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct PendingApprovalDecision {
    pub(crate) decision: CommandExecutionApprovalDecision,
    pub(crate) granted_permissions: Option<GrantedPermissionProfile>,
    pub(crate) permission_scope: PermissionGrantScope,
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
        requested_permissions: request.requested_permissions.clone(),
        available_decisions: vec![
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Accept,
            ),
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::AcceptForSession,
            ),
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Decline,
            ),
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Cancel,
            ),
        ],
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
            decision: CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Cancel,
            ),
            granted_permissions: None,
            permission_scope: PermissionGrantScope::Turn,
            reason: Some("approval request was cancelled".to_string()),
        },
        Err(_) => PendingApprovalDecision {
            decision: CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Decline,
            ),
            granted_permissions: None,
            permission_scope: PermissionGrantScope::Turn,
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

pub(crate) async fn approve_pending_approval(
    id: &str,
    decision: CommandExecutionApprovalDecision,
    granted_permissions: Option<GrantedPermissionProfile>,
) -> Result<bool, String> {
    let mut pending = pending_store().lock().await;
    let Some(entry) = pending.get_mut(id) else {
        return Ok(false);
    };
    if !entry.item.available_decisions.contains(&decision) {
        return Err("approval decision is not available for this request".to_string());
    }
    let granted_permissions = match (
        entry.item.requested_permissions.as_ref(),
        granted_permissions,
    ) {
        (Some(requested), Some(granted)) => {
            if !requested.allows_grant(&granted) {
                return Err("granted permissions exceed the request".to_string());
            }
            Some(granted)
        }
        (Some(requested), None) => Some(requested.clone().into()),
        (None, Some(_)) => {
            return Err(
                "approval supplied permissions for a request without an overlay".to_string(),
            )
        }
        (None, None) => None,
    };
    let permission_scope = if decision
        == CommandExecutionApprovalDecision::Simple(
            SimpleCommandExecutionApprovalDecision::AcceptForSession,
        ) {
        PermissionGrantScope::Session
    } else {
        PermissionGrantScope::Turn
    };
    let Some(tx) = entry.tx.take() else {
        return Ok(false);
    };
    Ok(tx
        .send(PendingApprovalDecision {
            decision,
            granted_permissions,
            permission_scope,
            reason: None,
        })
        .is_ok())
}

pub(crate) async fn deny_pending_approval(id: &str, reason: Option<String>) -> bool {
    resolve_pending_approval(
        id,
        PendingApprovalDecision {
            decision: CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Decline,
            ),
            granted_permissions: None,
            permission_scope: PermissionGrantScope::Turn,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::{ApprovalProjectKey, CommandApprovalRequest};
    use chatos_sandbox_contract::{AdditionalNetworkPermissions, RequestPermissionProfile};

    #[tokio::test]
    async fn approving_permission_request_defaults_to_the_exact_requested_grant() {
        let request_id = format!("request-{}", uuid::Uuid::new_v4());
        let request = CommandApprovalRequest {
            request_id: request_id.clone(),
            project_key: ApprovalProjectKey {
                owner_user_id: "owner".to_string(),
                device_id: "device".to_string(),
                workspace_id: "workspace".to_string(),
                project_id: None,
                project_root_relative_path: ".".to_string(),
                project_anchor_relative_path: None,
            },
            command: "curl".to_string(),
            args: Vec::new(),
            cwd: ".".to_string(),
            source: "test".to_string(),
            requested_permissions: Some(RequestPermissionProfile {
                file_system: None,
                network: Some(AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
            }),
            session_id: Some("session".to_string()),
        };
        let waiter = tokio::spawn(async move {
            request_pending_approval(&request, "high".to_string(), None).await
        });
        let id = loop {
            if let Some(item) = list_pending_approvals()
                .await
                .into_iter()
                .find(|item| item.request_id == request_id)
            {
                break item.id;
            }
            tokio::task::yield_now().await;
        };
        assert!(approve_pending_approval(
            id.as_str(),
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Accept,
            ),
            None,
        )
        .await
        .expect("resolve approval"));
        let decision = waiter.await.expect("waiter");
        assert_eq!(decision.permission_scope, PermissionGrantScope::Turn);
        assert_eq!(
            decision
                .granted_permissions
                .and_then(|grant| grant.network)
                .and_then(|network| network.enabled),
            Some(true)
        );
    }
}

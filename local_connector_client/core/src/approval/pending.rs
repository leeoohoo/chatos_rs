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
use super::types::{ApprovalConfirmationRequirement, CommandApprovalRequest, PendingApprovalItem};

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
    session_id: Option<String>,
    tx: Option<oneshot::Sender<PendingApprovalDecision>>,
}

struct PendingApprovalCleanup {
    id: String,
    armed: bool,
}

impl Drop for PendingApprovalCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let id = self.id.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            drop(runtime.spawn(async move {
                pending_store().lock().await.remove(id.as_str());
            }));
        }
    }
}

fn pending_item_for_request(
    id: String,
    request: &CommandApprovalRequest,
    risk: String,
    reason: Option<String>,
) -> PendingApprovalItem {
    let single_use_only = request.action_audit.as_ref().is_some_and(|audit| {
        matches!(
            audit.kind.as_str(),
            "computer_use" | "plugin_artifact_write" | "plugin_hook_workspace_write"
        )
    });
    let confirmation = computer_use_confirmation_requirement(request);
    let mut available_decisions = vec![CommandExecutionApprovalDecision::Simple(
        SimpleCommandExecutionApprovalDecision::Accept,
    )];
    if !single_use_only {
        available_decisions.push(CommandExecutionApprovalDecision::Simple(
            SimpleCommandExecutionApprovalDecision::AcceptForSession,
        ));
    }
    available_decisions.extend([
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Decline),
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Cancel),
    ]);
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
        action_audit: request.action_audit.clone(),
        confirmation,
        available_decisions,
    }
}

fn computer_use_confirmation_requirement(
    request: &CommandApprovalRequest,
) -> Option<ApprovalConfirmationRequirement> {
    let audit = request.action_audit.as_ref()?;
    if audit.kind != "computer_use" {
        return None;
    }
    let risk = match audit.operation.as_str() {
        "computer_type_text" => "sensitive_text_entry",
        "computer_restore_window_layout" => "multi_window_layout_restore",
        "computer_press_key" => {
            let key = audit_detail_value(audit, "key")?;
            let modifiers = audit_detail_value(audit, "modifiers").unwrap_or("none");
            if key == "enter" {
                "submit_or_activate"
            } else if key == "backspace" {
                "destructive_key"
            } else if modifiers != "none" {
                "application_shortcut"
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let code = Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase();
    Some(ApprovalConfirmationRequirement {
        kind: "typed_challenge".to_string(),
        risk: risk.to_string(),
        challenge: format!("CONFIRM-{code}"),
    })
}

fn audit_detail_value<'a>(
    audit: &'a super::types::ApprovalActionAudit,
    key: &str,
) -> Option<&'a str> {
    audit
        .details
        .iter()
        .find(|detail| detail.key == key)
        .map(|detail| detail.value.as_str())
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
        pending.insert(
            id.clone(),
            PendingApprovalState {
                item,
                session_id: request.session_id.clone(),
                tx: Some(tx),
            },
        );
    }
    let mut cleanup = PendingApprovalCleanup {
        id: id.clone(),
        armed: true,
    };

    let result = tokio::time::timeout(APPROVAL_TIMEOUT, rx).await;
    {
        let mut pending = pending_store().lock().await;
        pending.remove(id.as_str());
    }
    cleanup.armed = false;
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

pub(crate) async fn cancel_pending_approvals_for_session(
    session_id: &str,
    reason: impl Into<String>,
) -> usize {
    let reason = reason.into();
    let mut cancelled = 0;
    let mut pending = pending_store().lock().await;
    for entry in pending.values_mut() {
        if entry.session_id.as_deref() != Some(session_id) {
            continue;
        }
        let Some(tx) = entry.tx.take() else {
            continue;
        };
        if tx
            .send(PendingApprovalDecision {
                decision: CommandExecutionApprovalDecision::Simple(
                    SimpleCommandExecutionApprovalDecision::Cancel,
                ),
                granted_permissions: None,
                permission_scope: PermissionGrantScope::Turn,
                reason: Some(reason.clone()),
            })
            .is_ok()
        {
            cancelled += 1;
        }
    }
    cancelled
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
    confirmation_response: Option<&str>,
) -> Result<bool, String> {
    let mut pending = pending_store().lock().await;
    let Some(entry) = pending.get_mut(id) else {
        return Ok(false);
    };
    if !entry.item.available_decisions.contains(&decision) {
        return Err("approval decision is not available for this request".to_string());
    }
    if matches!(
        decision,
        CommandExecutionApprovalDecision::Simple(SimpleCommandExecutionApprovalDecision::Accept)
    ) && entry.item.confirmation.as_ref().is_some_and(|requirement| {
        confirmation_response.map(str::trim) != Some(requirement.challenge.as_str())
    }) {
        return Err(
            "high-risk Computer Use approval requires the exact typed confirmation challenge"
                .to_string(),
        );
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
    use crate::approval::{
        ApprovalActionAudit, ApprovalActionAuditDetail, ApprovalProjectKey, CommandApprovalRequest,
    };
    use chatos_sandbox_contract::{AdditionalNetworkPermissions, RequestPermissionProfile};

    fn computer_use_request(
        operation: &str,
        details: Vec<ApprovalActionAuditDetail>,
    ) -> CommandApprovalRequest {
        CommandApprovalRequest {
            request_id: format!("request-{}", uuid::Uuid::new_v4()),
            project_key: ApprovalProjectKey {
                owner_user_id: "owner".to_string(),
                device_id: "device".to_string(),
                workspace_id: "workspace".to_string(),
                project_id: None,
                project_root_relative_path: ".".to_string(),
                project_anchor_relative_path: None,
            },
            command: operation.to_string(),
            args: Vec::new(),
            redact_arguments_in_history: operation == "computer_type_text",
            cwd: ".".to_string(),
            source: "plugin_computer_use".to_string(),
            requested_permissions: None,
            session_id: Some(format!("session-{}", uuid::Uuid::new_v4())),
            action_audit: Some(ApprovalActionAudit {
                kind: "computer_use".to_string(),
                operation: operation.to_string(),
                details,
                privacy: None,
                safety: None,
                recovery: None,
            }),
        }
    }

    fn audit_detail(key: &str, value: &str) -> ApprovalActionAuditDetail {
        ApprovalActionAuditDetail {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn high_risk_computer_use_actions_receive_typed_challenges() {
        let type_text = computer_use_request("computer_type_text", Vec::new());
        let type_text_confirmation = computer_use_confirmation_requirement(&type_text)
            .expect("text entry confirmation requirement");
        assert_eq!(type_text_confirmation.kind, "typed_challenge");
        assert_eq!(type_text_confirmation.risk, "sensitive_text_entry");
        assert!(type_text_confirmation.challenge.starts_with("CONFIRM-"));

        let restore = computer_use_request("computer_restore_window_layout", Vec::new());
        let restore_confirmation = computer_use_confirmation_requirement(&restore)
            .expect("window layout restore confirmation requirement");
        assert_eq!(restore_confirmation.kind, "typed_challenge");
        assert_eq!(restore_confirmation.risk, "multi_window_layout_restore");
        assert!(restore_confirmation.challenge.starts_with("CONFIRM-"));

        for (key, modifiers, expected_risk) in [
            ("enter", "none", Some("submit_or_activate")),
            ("backspace", "none", Some("destructive_key")),
            ("escape", "command", Some("application_shortcut")),
            ("escape", "none", None),
        ] {
            let request = computer_use_request(
                "computer_press_key",
                vec![
                    audit_detail("key", key),
                    audit_detail("modifiers", modifiers),
                ],
            );
            assert_eq!(
                computer_use_confirmation_requirement(&request)
                    .as_ref()
                    .map(|confirmation| confirmation.risk.as_str()),
                expected_risk,
                "key={key}, modifiers={modifiers}"
            );
        }
    }

    #[tokio::test]
    async fn high_risk_computer_use_approval_requires_exact_one_time_challenge() {
        let request = computer_use_request("computer_type_text", Vec::new());
        let request_id = request.request_id.clone();
        let waiter = tokio::spawn(async move {
            request_pending_approval(&request, "high".to_string(), None).await
        });
        let item = loop {
            if let Some(item) = list_pending_approvals()
                .await
                .into_iter()
                .find(|item| item.request_id == request_id)
            {
                break item;
            }
            tokio::task::yield_now().await;
        };
        let accept = CommandExecutionApprovalDecision::Simple(
            SimpleCommandExecutionApprovalDecision::Accept,
        );
        let accept_for_session = CommandExecutionApprovalDecision::Simple(
            SimpleCommandExecutionApprovalDecision::AcceptForSession,
        );
        assert!(item.available_decisions.contains(&accept));
        assert!(!item.available_decisions.contains(&accept_for_session));
        let challenge = item
            .confirmation
            .as_ref()
            .expect("typed confirmation requirement")
            .challenge
            .clone();

        let missing = approve_pending_approval(item.id.as_str(), accept.clone(), None, None)
            .await
            .expect_err("missing confirmation must fail");
        assert!(missing.contains("exact typed confirmation"));
        let wrong = approve_pending_approval(
            item.id.as_str(),
            accept.clone(),
            None,
            Some("CONFIRM-WRONG"),
        )
        .await
        .expect_err("wrong confirmation must fail");
        assert!(wrong.contains("exact typed confirmation"));
        assert!(
            approve_pending_approval(item.id.as_str(), accept, None, Some(challenge.as_str()),)
                .await
                .expect("exact confirmation resolves approval")
        );

        let decision = waiter.await.expect("approval waiter");
        assert_eq!(decision.permission_scope, PermissionGrantScope::Turn);
    }

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
            redact_arguments_in_history: false,
            cwd: ".".to_string(),
            source: "test".to_string(),
            requested_permissions: Some(RequestPermissionProfile {
                file_system: None,
                network: Some(AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
            }),
            session_id: Some("session".to_string()),
            action_audit: None,
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

    #[tokio::test]
    async fn cancelling_a_session_wakes_its_pending_approval() {
        let session_id = format!("session-{}", uuid::Uuid::new_v4());
        let request_id = format!("request-{}", uuid::Uuid::new_v4());
        let action_audit = ApprovalActionAudit {
            kind: "computer_use".to_string(),
            operation: "computer_click".to_string(),
            details: vec![ApprovalActionAuditDetail {
                key: "point".to_string(),
                value: "10, 20".to_string(),
            }],
            privacy: None,
            safety: Some("display_identity_and_geometry_revalidated".to_string()),
            recovery: None,
        };
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
            command: "computer_click".to_string(),
            args: vec!["--x=10".to_string(), "--y=20".to_string()],
            redact_arguments_in_history: false,
            cwd: ".".to_string(),
            source: "plugin_computer_use".to_string(),
            requested_permissions: None,
            session_id: Some(session_id.clone()),
            action_audit: Some(action_audit.clone()),
        };
        let pending = tokio::spawn(async move {
            request_pending_approval(&request, "high".to_string(), None).await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(item) = list_pending_approvals()
                    .await
                    .into_iter()
                    .find(|item| item.request_id == request_id)
                {
                    assert_eq!(item.action_audit.as_ref(), Some(&action_audit));
                    assert!(!item.available_decisions.contains(
                        &CommandExecutionApprovalDecision::Simple(
                            SimpleCommandExecutionApprovalDecision::AcceptForSession,
                        ),
                    ));
                    assert!(item.confirmation.is_none());
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending desktop approval");

        assert_eq!(
            cancel_pending_approvals_for_session(&session_id, "emergency stop").await,
            1
        );
        let decision = pending.await.expect("pending approval task");
        assert_eq!(decision.reason.as_deref(), Some("emergency stop"));
        assert_eq!(
            decision.decision,
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Cancel
            )
        );
    }

    #[tokio::test]
    async fn aborting_an_execute_future_removes_its_pending_approval() {
        let session_id = format!("session-{}", uuid::Uuid::new_v4());
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
            command: "computer_press_key".to_string(),
            args: vec!["--key=escape".to_string()],
            redact_arguments_in_history: false,
            cwd: ".".to_string(),
            source: "plugin_computer_use".to_string(),
            requested_permissions: None,
            session_id: Some(session_id),
            action_audit: None,
        };
        let pending = tokio::spawn(async move {
            request_pending_approval(&request, "high".to_string(), None).await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if list_pending_approvals()
                    .await
                    .iter()
                    .any(|item| item.request_id == request_id)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending approval before abort");
        pending.abort();
        let _ = pending.await;
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if !list_pending_approvals()
                    .await
                    .iter()
                    .any(|item| item.request_id == request_id)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("aborted approval cleanup");
    }
}

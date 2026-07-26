// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use chatos_sandbox_contract::{
    CommandExecutionApprovalDecision, GrantedPermissionProfile, PermissionGrantScope,
    SimpleCommandExecutionApprovalDecision,
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{local_now_rfc3339, tracing_stdout, LocalState};
use crate::{relay::RelayRequest, WorkspaceState};

use super::fingerprint::normalized_command;
use super::pending::request_pending_approval;
use super::risk::classify_command_request;
use super::types::{
    ApprovalDecision, ApprovalHistoryEntry, ApprovalMode, ApprovalProjectKey, ApprovalSource,
    CommandApprovalRequest, WhitelistCwdScope,
};
use super::whitelist::{build_whitelist_entry, find_matching_whitelist};
use super::{
    finish_in_progress_approval, run_auto_approval_agent, start_in_progress_approval,
    AutoApprovalDecision,
};

const MAX_APPROVAL_HISTORY_ENTRIES: usize = 1_000;

#[derive(Debug, Clone)]
pub(crate) struct CommandApprovalService {
    state_path: PathBuf,
    state: Arc<RwLock<LocalState>>,
}

impl CommandApprovalService {
    pub(crate) fn new(state_path: PathBuf, state: Arc<RwLock<LocalState>>) -> Self {
        Self { state_path, state }
    }

    pub(crate) async fn approve(
        &self,
        request: CommandApprovalRequest,
    ) -> Result<ApprovalDecision> {
        self.approve_with_optional_mode(request, None).await
    }

    pub(crate) async fn approve_with_mode(
        &self,
        request: CommandApprovalRequest,
        mode: ApprovalMode,
    ) -> Result<ApprovalDecision> {
        self.approve_with_options(request, Some(mode), true, true)
            .await
    }

    pub(crate) async fn approve_interactive(
        &self,
        request: CommandApprovalRequest,
    ) -> Result<ApprovalDecision> {
        self.approve_with_options(request, Some(ApprovalMode::RequestApproval), false, false)
            .await
    }

    async fn approve_with_optional_mode(
        &self,
        request: CommandApprovalRequest,
        forced_mode: Option<ApprovalMode>,
    ) -> Result<ApprovalDecision> {
        self.approve_with_options(request, forced_mode, true, true)
            .await
    }

    async fn approve_with_options(
        &self,
        request: CommandApprovalRequest,
        forced_mode: Option<ApprovalMode>,
        allow_whitelist: bool,
        allow_session_approval: bool,
    ) -> Result<ApprovalDecision> {
        let state_snapshot = self.state.read().await.clone();
        let mode =
            forced_mode.unwrap_or_else(|| approval_mode_for_request(&state_snapshot, &request));
        let risk = classify_command_request(
            normalized_command(request.command.as_str(), request.args.as_slice()).as_str(),
            request.requested_permissions.as_ref(),
        );

        if allow_session_approval && session_approval_matches(&request).await {
            let decision = ApprovalDecision::Approved {
                source: ApprovalSource::User,
                reason: Some("matched session-scoped approval".to_string()),
                whitelist_entry_id: None,
                granted_permissions: requested_grant(&request),
                permission_scope: PermissionGrantScope::Session,
            };
            self.append_history(&request, mode, &decision, risk.level, risk.reason)
                .await?;
            return Ok(decision);
        }

        if let Some(entry) = (allow_whitelist && request.requested_permissions.is_none())
            .then(|| {
                find_matching_whitelist(
                    state_snapshot.approval.whitelist.as_slice(),
                    &request.project_key,
                    request.command.as_str(),
                    request.args.as_slice(),
                    request.cwd.as_str(),
                )
            })
            .flatten()
        {
            let decision = ApprovalDecision::Approved {
                source: ApprovalSource::Whitelist,
                reason: Some("matched project command whitelist".to_string()),
                whitelist_entry_id: Some(entry.id.clone()),
                granted_permissions: None,
                permission_scope: PermissionGrantScope::Turn,
            };
            self.append_history(&request, mode, &decision, risk.level, risk.reason)
                .await?;
            return Ok(decision);
        }

        let in_progress_id = if mode == ApprovalMode::AutoApproval {
            Some(
                start_in_progress_approval(
                    &request,
                    risk.level.clone(),
                    Some("AI 正在结合项目文件审核这条命令".to_string()),
                )
                .await,
            )
        } else {
            None
        };

        let decision_result = match mode {
            ApprovalMode::FullControl => Ok(ApprovalDecision::Approved {
                source: ApprovalSource::FullControl,
                reason: Some("full control mode".to_string()),
                whitelist_entry_id: None,
                granted_permissions: requested_grant(&request),
                permission_scope: PermissionGrantScope::Turn,
            }),
            ApprovalMode::AutoApproval => self.auto_approve(&state_snapshot, &request, &risk).await,
            ApprovalMode::RequestApproval => self.request_user_approval(&request, &risk).await,
        };
        let decision = match decision_result {
            Ok(decision) => decision,
            Err(err) => {
                if let Some(id) = in_progress_id.as_deref() {
                    finish_in_progress_approval(id).await;
                }
                return Err(err);
            }
        };

        let append_result = self
            .append_history(&request, mode, &decision, risk.level, risk.reason)
            .await;
        if let Some(id) = in_progress_id.as_deref() {
            finish_in_progress_approval(id).await;
        }
        append_result?;
        Ok(decision)
    }

    async fn auto_approve(
        &self,
        state_snapshot: &LocalState,
        request: &CommandApprovalRequest,
        risk: &super::risk::RiskSummary,
    ) -> Result<ApprovalDecision> {
        match run_auto_approval_agent(
            state_snapshot,
            self.state_path.as_path(),
            request,
            risk.level.as_str(),
            risk.reason.as_deref(),
        )
        .await
        {
            Ok(AutoApprovalDecision::Approved { reason }) => Ok(ApprovalDecision::Approved {
                source: ApprovalSource::Ai,
                reason: Some(reason),
                whitelist_entry_id: None,
                granted_permissions: requested_grant(request),
                permission_scope: PermissionGrantScope::Turn,
            }),
            Ok(AutoApprovalDecision::Denied { reason }) => Ok(ApprovalDecision::Denied {
                source: ApprovalSource::Ai,
                reason,
            }),
            Ok(AutoApprovalDecision::AskUser { reason }) => {
                self.request_user_approval_with_reason(request, risk, Some(reason))
                    .await
            }
            Err(err) => Ok(ApprovalDecision::Denied {
                source: ApprovalSource::StaticRule,
                reason: format!("AI approval unavailable: {err}"),
            }),
        }
    }

    async fn request_user_approval(
        &self,
        request: &CommandApprovalRequest,
        risk: &super::risk::RiskSummary,
    ) -> Result<ApprovalDecision> {
        self.request_user_approval_with_reason(request, risk, None)
            .await
    }

    async fn request_user_approval_with_reason(
        &self,
        request: &CommandApprovalRequest,
        risk: &super::risk::RiskSummary,
        reason_override: Option<String>,
    ) -> Result<ApprovalDecision> {
        let reason = reason_override.or_else(|| risk.reason.clone());
        let pending = request_pending_approval(request, risk.level.clone(), reason).await;
        match pending.decision {
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Accept,
            ) => Ok(ApprovalDecision::Approved {
                source: ApprovalSource::User,
                reason: Some("approved by user".to_string()),
                whitelist_entry_id: None,
                granted_permissions: pending.granted_permissions,
                permission_scope: pending.permission_scope,
            }),
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::AcceptForSession,
            ) => {
                remember_session_approval(request).await?;
                Ok(ApprovalDecision::Approved {
                    source: ApprovalSource::User,
                    reason: Some("approved by user for this session".to_string()),
                    whitelist_entry_id: None,
                    granted_permissions: pending.granted_permissions,
                    permission_scope: PermissionGrantScope::Session,
                })
            }
            CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment { .. } => {
                let entry_id = self
                    .add_whitelist_entry(request, WhitelistCwdScope::Project, ApprovalSource::User)
                    .await?;
                Ok(ApprovalDecision::Approved {
                    source: ApprovalSource::User,
                    reason: Some("approved with an exec policy amendment".to_string()),
                    whitelist_entry_id: Some(entry_id),
                    granted_permissions: pending.granted_permissions,
                    permission_scope: pending.permission_scope,
                })
            }
            CommandExecutionApprovalDecision::ApplyNetworkPolicyAmendment { .. } => {
                Ok(ApprovalDecision::Denied {
                    source: ApprovalSource::StaticRule,
                    reason: "persistent network policy amendments are not enabled yet".to_string(),
                })
            }
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::Decline
                | SimpleCommandExecutionApprovalDecision::Cancel,
            ) => Ok(ApprovalDecision::Denied {
                source: ApprovalSource::User,
                reason: pending
                    .reason
                    .unwrap_or_else(|| "denied by user".to_string()),
            }),
        }
    }

    async fn add_whitelist_entry(
        &self,
        request: &CommandApprovalRequest,
        cwd_scope: WhitelistCwdScope,
        created_by: ApprovalSource,
    ) -> Result<String> {
        let entry = build_whitelist_entry(
            request.project_key.clone(),
            request.command.as_str(),
            request.args.as_slice(),
            request.cwd.as_str(),
            cwd_scope,
            created_by,
        );
        let id = entry.id.clone();
        let mut state = self.state.write().await;
        state.approval.whitelist.push(entry);
        save_state(&state, self.state_path.as_path());
        Ok(id)
    }

    async fn append_history(
        &self,
        request: &CommandApprovalRequest,
        mode: ApprovalMode,
        decision: &ApprovalDecision,
        risk: String,
        risk_reason: Option<String>,
    ) -> Result<()> {
        let (decision_text, decision_source, reason, whitelist_entry_id, permission_scope) =
            match decision {
                ApprovalDecision::Approved {
                    source,
                    reason,
                    whitelist_entry_id,
                    permission_scope,
                    ..
                } => (
                    "approved".to_string(),
                    *source,
                    reason.clone().or(risk_reason),
                    whitelist_entry_id.clone(),
                    Some(*permission_scope),
                ),
                ApprovalDecision::Denied { source, reason } => (
                    "denied".to_string(),
                    *source,
                    Some(reason.clone()),
                    None,
                    None,
                ),
            };
        let normalized_command =
            normalized_command(request.command.as_str(), request.args.as_slice());
        let history_normalized_command = if request.redact_arguments_in_history {
            format!(
                "{} [arguments redacted; count={}; sha256={}]",
                request.command.trim(),
                request.args.len(),
                hex::encode(Sha256::digest(normalized_command.as_bytes()))
            )
        } else {
            normalized_command
        };
        let entry = ApprovalHistoryEntry {
            id: format!("approval-history-{}", Uuid::new_v4()),
            request_id: request.request_id.clone(),
            project_key: request.project_key.clone(),
            command: request.command.clone(),
            normalized_command: history_normalized_command,
            cwd: request.cwd.clone(),
            source: request.source.clone(),
            mode,
            decision: decision_text,
            decision_source,
            risk,
            reason,
            whitelist_entry_id,
            permission_scope,
            action_audit: request.action_audit.clone(),
            created_at: local_now_rfc3339(),
        };
        let mut state = self.state.write().await;
        state.approval.history.push(entry);
        let overflow = state
            .approval
            .history
            .len()
            .saturating_sub(MAX_APPROVAL_HISTORY_ENTRIES);
        if overflow > 0 {
            state.approval.history.drain(0..overflow);
        }
        save_state(&state, self.state_path.as_path());
        Ok(())
    }
}

fn requested_grant(request: &CommandApprovalRequest) -> Option<GrantedPermissionProfile> {
    request
        .requested_permissions
        .clone()
        .map(GrantedPermissionProfile::from)
}

fn session_approval_store() -> &'static Mutex<BTreeMap<String, BTreeSet<String>>> {
    static STORE: OnceLock<Mutex<BTreeMap<String, BTreeSet<String>>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

async fn session_approval_matches(request: &CommandApprovalRequest) -> bool {
    let Some(session_id) = request.session_id.as_deref() else {
        return false;
    };
    let key = session_approval_key(request);
    session_approval_store()
        .lock()
        .await
        .get(session_id)
        .is_some_and(|entries| entries.contains(key.as_str()))
}

async fn remember_session_approval(request: &CommandApprovalRequest) -> Result<()> {
    let Some(session_id) = request.session_id.as_deref() else {
        return Ok(());
    };
    session_approval_store()
        .lock()
        .await
        .entry(session_id.to_string())
        .or_default()
        .insert(session_approval_key(request));
    Ok(())
}

pub(crate) async fn clear_session_approvals(session_id: &str) {
    session_approval_store().lock().await.remove(session_id);
}

fn session_approval_key(request: &CommandApprovalRequest) -> String {
    let permissions = request
        .requested_permissions
        .as_ref()
        .and_then(|permissions| serde_json::to_string(permissions).ok())
        .unwrap_or_default();
    format!(
        "{}\n{}\n{}",
        normalized_command(request.command.as_str(), request.args.as_slice()),
        request.cwd,
        permissions
    )
}

fn approval_mode_for_request(state: &LocalState, request: &CommandApprovalRequest) -> ApprovalMode {
    state
        .approval
        .projects
        .iter()
        .find(|project| project.project_key == request.project_key)
        .and_then(|project| project.mode)
        .unwrap_or(state.approval.default_mode)
}

fn save_state(state: &LocalState, path: &std::path::Path) {
    if let Err(err) = state.save(path) {
        tracing_stdout(format!("save approval state failed: {err}").as_str());
    }
}

pub(crate) fn approval_project_key_from_request(
    state: &LocalState,
    request: &RelayRequest,
    workspace: &WorkspaceState,
    project_root_relative_path: impl Into<String>,
) -> ApprovalProjectKey {
    let owner_user_id = request
        .owner_user_id
        .clone()
        .or_else(|| {
            state
                .auth
                .as_ref()
                .and_then(|auth| auth.user.as_ref().map(|user| user.id.clone()))
        })
        .or_else(|| state.paired_user_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let device_id = request
        .device_id
        .clone()
        .or_else(|| state.device_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let project_id = header_value(request, "x-local-connector-project-id")
        .or_else(|| header_value(request, "x-project-id"));
    let project_root_relative_path = header_value(request, "x-local-connector-project-root")
        .unwrap_or_else(|| project_root_relative_path.into());
    let project_anchor_relative_path = header_value(request, "x-local-connector-project-anchor");
    ApprovalProjectKey {
        owner_user_id,
        device_id,
        workspace_id: workspace.id.clone(),
        project_id,
        project_root_relative_path,
        project_anchor_relative_path,
    }
}

pub(crate) fn approval_project_key_for_relay_scope(
    state: &LocalState,
    request: &RelayRequest,
) -> ApprovalProjectKey {
    let owner_user_id = request
        .owner_user_id
        .clone()
        .or_else(|| {
            state
                .auth
                .as_ref()
                .and_then(|auth| auth.user.as_ref().map(|user| user.id.clone()))
        })
        .or_else(|| state.paired_user_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let device_id = request
        .device_id
        .clone()
        .or_else(|| state.device_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    ApprovalProjectKey {
        owner_user_id,
        device_id,
        workspace_id: request.workspace_id.trim().to_string(),
        project_id: header_value(request, "x-local-connector-project-id")
            .or_else(|| header_value(request, "x-project-id")),
        project_root_relative_path: header_value(request, "x-local-connector-project-root")
            .unwrap_or_else(|| ".".to_string()),
        project_anchor_relative_path: header_value(request, "x-local-connector-project-anchor"),
    }
}

fn header_value(request: &RelayRequest, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::approval::{ApprovalActionAudit, ApprovalActionAuditDetail};
    use chatos_sandbox_contract::{AdditionalNetworkPermissions, RequestPermissionProfile};
    use tokio::sync::RwLock;

    fn request(session_id: &str, network: bool) -> CommandApprovalRequest {
        CommandApprovalRequest {
            request_id: format!("request-{session_id}"),
            project_key: ApprovalProjectKey {
                owner_user_id: "owner".to_string(),
                device_id: "device".to_string(),
                workspace_id: "workspace".to_string(),
                project_id: Some("project".to_string()),
                project_root_relative_path: ".".to_string(),
                project_anchor_relative_path: None,
            },
            command: "cargo".to_string(),
            args: vec!["test".to_string()],
            redact_arguments_in_history: false,
            cwd: ".".to_string(),
            source: "test".to_string(),
            requested_permissions: network.then_some(RequestPermissionProfile {
                file_system: None,
                network: Some(AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
            }),
            session_id: Some(session_id.to_string()),
            action_audit: None,
        }
    }

    #[tokio::test]
    async fn session_approval_matches_only_the_same_command_and_permission_request() {
        let session_id = format!("session-{}", uuid::Uuid::new_v4());
        let approved = request(session_id.as_str(), true);
        remember_session_approval(&approved)
            .await
            .expect("remember session approval");

        assert!(session_approval_matches(&approved).await);
        assert!(!session_approval_matches(&request(session_id.as_str(), false)).await);
        assert!(!session_approval_matches(&request("another-session", true)).await);
    }

    #[tokio::test]
    async fn interactive_approval_cannot_be_bypassed_by_full_control_mode() {
        let session_id = format!("desktop-session-{}", uuid::Uuid::new_v4());
        let request_id = format!("desktop-request-{}", uuid::Uuid::new_v4());
        let mut state = LocalState::default();
        state.approval.default_mode = ApprovalMode::FullControl;
        let state = Arc::new(RwLock::new(state));
        let temp = tempfile::tempdir().expect("approval state directory");
        let service = CommandApprovalService::new(temp.path().join("state.json"), state);
        let mut desktop_request = request(session_id.as_str(), false);
        desktop_request.request_id = request_id.clone();
        desktop_request.command = "computer_press_key".to_string();
        desktop_request.args = vec!["--key=enter".to_string()];
        desktop_request.source = "plugin_computer_use".to_string();

        let approval = tokio::spawn(async move {
            service
                .approve_interactive(desktop_request)
                .await
                .expect("interactive approval result")
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if crate::approval::list_pending_approvals()
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
        .expect("interactive approval must remain pending");
        assert_eq!(
            crate::approval::cancel_pending_approvals_for_session(
                session_id.as_str(),
                "test emergency stop",
            )
            .await,
            1
        );
        assert!(matches!(
            approval.await.expect("interactive approval task"),
            ApprovalDecision::Denied { .. }
        ));
    }

    #[tokio::test]
    async fn sensitive_interactive_arguments_are_redacted_from_persisted_history() {
        let secret = "private text for the focused field";
        let state = Arc::new(RwLock::new(LocalState::default()));
        let temp = tempfile::tempdir().expect("approval state directory");
        let service = CommandApprovalService::new(temp.path().join("state.json"), state.clone());
        let mut desktop_request = request("desktop-redaction", false);
        desktop_request.command = "computer_type_text".to_string();
        desktop_request.args = vec![format!(
            "--text-json={}",
            serde_json::to_string(secret).unwrap()
        )];
        desktop_request.redact_arguments_in_history = true;
        desktop_request.action_audit = Some(ApprovalActionAudit {
            kind: "computer_use".to_string(),
            operation: "computer_type_text".to_string(),
            details: vec![ApprovalActionAuditDetail {
                key: "character_count".to_string(),
                value: secret.chars().count().to_string(),
            }],
            privacy: Some("text_redacted_from_persistent_history".to_string()),
            safety: Some("focused_target_revalidated_before_input".to_string()),
            recovery: None,
        });
        service
            .append_history(
                &desktop_request,
                ApprovalMode::RequestApproval,
                &ApprovalDecision::Approved {
                    source: ApprovalSource::User,
                    reason: Some("approved by user".to_string()),
                    whitelist_entry_id: None,
                    granted_permissions: None,
                    permission_scope: PermissionGrantScope::Turn,
                },
                "high".to_string(),
                None,
            )
            .await
            .expect("append redacted approval history");

        let state = state.read().await;
        let history = state.approval.history.last().expect("approval history");
        assert_eq!(history.command, "computer_type_text");
        assert!(history.normalized_command.contains("arguments redacted"));
        assert!(history.normalized_command.contains("sha256="));
        assert!(!history.normalized_command.contains(secret));
        assert_eq!(
            history
                .action_audit
                .as_ref()
                .map(|audit| audit.operation.as_str()),
            Some("computer_type_text")
        );
        assert!(!serde_json::to_string(&history.action_audit)
            .unwrap()
            .contains(secret));
    }
}

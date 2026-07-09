// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{local_now_rfc3339, tracing_stdout, LocalState};
use crate::{relay::RelayRequest, WorkspaceState};

use super::fingerprint::normalized_command;
use super::pending::request_pending_approval;
use super::risk::classify_command;
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

#[derive(Clone)]
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
        let state_snapshot = self.state.read().await.clone();
        let mode = approval_mode_for_request(&state_snapshot, &request);
        let risk = classify_command(
            normalized_command(request.command.as_str(), request.args.as_slice()).as_str(),
        );

        if let Some(entry) = find_matching_whitelist(
            state_snapshot.approval.whitelist.as_slice(),
            &request.project_key,
            request.command.as_str(),
            request.args.as_slice(),
            request.cwd.as_str(),
        ) {
            let decision = ApprovalDecision::Approved {
                source: ApprovalSource::Whitelist,
                reason: Some("matched project command whitelist".to_string()),
                whitelist_entry_id: Some(entry.id.clone()),
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
            request,
            risk.level.as_str(),
            risk.reason.as_deref(),
        )
        .await
        {
            Ok(AutoApprovalDecision::Approved {
                reason,
                remember_allow,
            }) => {
                let whitelist_entry_id = if remember_allow {
                    Some(
                        self.add_whitelist_entry(
                            request,
                            WhitelistCwdScope::Project,
                            ApprovalSource::Ai,
                        )
                        .await?,
                    )
                } else {
                    None
                };
                Ok(ApprovalDecision::Approved {
                    source: ApprovalSource::Ai,
                    reason: Some(reason),
                    whitelist_entry_id,
                })
            }
            Ok(AutoApprovalDecision::Denied { reason }) => Ok(ApprovalDecision::Denied {
                source: ApprovalSource::Ai,
                reason,
            }),
            Ok(AutoApprovalDecision::AskUser { reason }) => Ok(ApprovalDecision::Denied {
                source: ApprovalSource::Ai,
                reason,
            }),
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
        if pending.approved {
            if pending.remember_allow {
                let entry_id = self
                    .add_whitelist_entry(request, WhitelistCwdScope::Project, ApprovalSource::User)
                    .await?;
                Ok(ApprovalDecision::Approved {
                    source: ApprovalSource::User,
                    reason: Some("approved by user".to_string()),
                    whitelist_entry_id: Some(entry_id),
                })
            } else {
                Ok(ApprovalDecision::Approved {
                    source: ApprovalSource::User,
                    reason: Some("approved by user".to_string()),
                    whitelist_entry_id: None,
                })
            }
        } else {
            Ok(ApprovalDecision::Denied {
                source: ApprovalSource::User,
                reason: pending
                    .reason
                    .unwrap_or_else(|| "denied by user".to_string()),
            })
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
        let (decision_text, decision_source, reason, whitelist_entry_id) = match decision {
            ApprovalDecision::Approved {
                source,
                reason,
                whitelist_entry_id,
            } => (
                "approved".to_string(),
                *source,
                reason.clone().or(risk_reason),
                whitelist_entry_id.clone(),
            ),
            ApprovalDecision::Denied { source, reason } => {
                ("denied".to_string(), *source, Some(reason.clone()), None)
            }
        };
        let entry = ApprovalHistoryEntry {
            id: format!("approval-history-{}", Uuid::new_v4()),
            request_id: request.request_id.clone(),
            project_key: request.project_key.clone(),
            command: request.command.clone(),
            normalized_command: normalized_command(
                request.command.as_str(),
                request.args.as_slice(),
            ),
            cwd: request.cwd.clone(),
            source: request.source.clone(),
            mode,
            decision: decision_text,
            decision_source,
            risk,
            reason,
            whitelist_entry_id,
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

fn header_value(request: &RelayRequest, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

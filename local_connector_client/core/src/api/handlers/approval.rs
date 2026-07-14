// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::approval::{
    approve_pending_approval, deny_pending_approval, list_in_progress_approvals,
    list_pending_approvals, ApprovalMode, ProjectApprovalState,
};
use crate::{local_now_rfc3339, LocalRuntime};

use super::super::types::{LocalApiError, ResolveApprovalRequest, UpdateApprovalSettingsRequest};

pub(crate) async fn local_approval_settings(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(approval_settings_payload(&state.approval)))
}

pub(crate) async fn local_update_approval_settings(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<UpdateApprovalSettingsRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let mut state = runtime.state.write().await;
    let current = state.approval.clone();
    validate_approval_settings_update(&req, &current)?;

    let mut next = current.clone();
    if let Some(default_mode) = req.default_mode {
        next.default_mode = default_mode;
    }
    if let Some(projects) = req.projects {
        next.projects = projects;
    }
    if let Some(mut ai) = req.ai {
        if ai.api_key.is_none() {
            ai.api_key = current.ai.api_key.clone();
        }
        next.ai = ai;
    }
    if let Some(memory) = req.memory {
        next.memory = memory;
    }
    if approval_settings_changed(&current, &next) {
        next.settings_revision = Some(format!("local-{}", local_now_rfc3339()));
    }

    state.approval = next;
    state.save(runtime.state_path.as_path())?;
    Ok(Json(approval_settings_payload(&state.approval)))
}

fn validate_approval_settings_update(
    req: &UpdateApprovalSettingsRequest,
    current: &crate::approval::ApprovalState,
) -> Result<(), LocalApiError> {
    if let Some(default_mode) = req.default_mode {
        if default_mode != current.default_mode
            && approval_mode_requires_risk_ack(default_mode)
            && !req.risk_acknowledged
        {
            return Err(LocalApiError::conflict_code(
                "approval_risk_ack_required",
                "switching to this approval mode requires explicit risk acknowledgement",
            ));
        }
    }
    if let Some(projects) = req.projects.as_deref() {
        if approval_projects_require_risk_ack(projects, current.projects.as_slice())
            && !req.risk_acknowledged
        {
            return Err(LocalApiError::conflict_code(
                "approval_risk_ack_required",
                "saving elevated project approval modes requires explicit risk acknowledgement",
            ));
        }
    }
    Ok(())
}

pub(crate) async fn local_pending_approvals() -> Result<Json<Value>, LocalApiError> {
    Ok(Json(json!({
        "items": list_pending_approvals().await,
        "reviewing": list_in_progress_approvals().await,
    })))
}

pub(crate) async fn local_approve_pending_approval(
    Path(id): Path<String>,
    Json(req): Json<ResolveApprovalRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let remember_allow = req.remember_allow.unwrap_or(false);
    if remember_allow && !req.risk_acknowledged {
        return Err(LocalApiError::conflict_code(
            "approval_risk_ack_required",
            "remembering an approval requires explicit risk acknowledgement",
        ));
    }
    let ok = approve_pending_approval(id.as_str(), remember_allow).await;
    if !ok {
        return Err(LocalApiError::bad_request("pending approval not found"));
    }
    Ok(Json(json!({ "ok": true })))
}

fn approval_settings_payload(state: &crate::approval::ApprovalState) -> Value {
    let mut value = json!(state);
    if let Some(ai) = value.get_mut("ai").and_then(Value::as_object_mut) {
        let has_api_key = state
            .ai
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        ai.remove("api_key");
        ai.insert("has_api_key".to_string(), Value::Bool(has_api_key));
    }
    value
}

fn approval_mode_requires_risk_ack(mode: ApprovalMode) -> bool {
    matches!(mode, ApprovalMode::AutoApproval | ApprovalMode::FullControl)
}

fn approval_projects_require_risk_ack(
    projects: &[ProjectApprovalState],
    current_projects: &[ProjectApprovalState],
) -> bool {
    projects
        .iter()
        .filter_map(|project| project.mode.map(|mode| (project, mode)))
        .any(|(project, mode)| {
            let current_mode = current_projects
                .iter()
                .find(|current| current.project_key == project.project_key)
                .and_then(|current| current.mode)
                .unwrap_or(ApprovalMode::RequestApproval);
            approval_mode_requires_risk_ack(mode)
                && approval_mode_rank(mode) > approval_mode_rank(current_mode)
        })
}

fn approval_mode_rank(mode: ApprovalMode) -> u8 {
    match mode {
        ApprovalMode::RequestApproval => 0,
        ApprovalMode::AutoApproval => 1,
        ApprovalMode::FullControl => 2,
    }
}

fn approval_settings_changed(
    current: &crate::approval::ApprovalState,
    next: &crate::approval::ApprovalState,
) -> bool {
    current.default_mode != next.default_mode
        || current.projects != next.projects
        || current.ai != next.ai
        || current.memory != next.memory
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::ApprovalProjectKey;

    fn project(mode: Option<ApprovalMode>) -> ProjectApprovalState {
        ProjectApprovalState {
            project_key: ApprovalProjectKey {
                owner_user_id: "owner".to_string(),
                device_id: "device".to_string(),
                workspace_id: "workspace".to_string(),
                project_id: None,
                project_root_relative_path: ".".to_string(),
                project_anchor_relative_path: None,
            },
            mode,
            ai_enabled: false,
            updated_at: "2026-07-15T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn elevated_approval_modes_require_risk_acknowledgement() {
        assert!(!approval_mode_requires_risk_ack(
            ApprovalMode::RequestApproval
        ));
        assert!(approval_mode_requires_risk_ack(ApprovalMode::AutoApproval));
        assert!(approval_mode_requires_risk_ack(ApprovalMode::FullControl));
    }

    #[test]
    fn approval_settings_validation_rejects_global_elevation_without_ack() {
        let req = UpdateApprovalSettingsRequest {
            default_mode: Some(ApprovalMode::AutoApproval),
            projects: None,
            ai: None,
            memory: None,
            risk_acknowledged: false,
        };

        let err =
            validate_approval_settings_update(&req, &crate::approval::ApprovalState::default())
                .expect_err("auto approval requires acknowledgement");

        assert_eq!(
            err.message(),
            "switching to this approval mode requires explicit risk acknowledgement"
        );
    }

    #[test]
    fn approval_settings_validation_allows_repeating_current_elevated_mode_without_ack() {
        let mut state = crate::approval::ApprovalState::default();
        state.default_mode = ApprovalMode::FullControl;
        let req = UpdateApprovalSettingsRequest {
            default_mode: Some(ApprovalMode::FullControl),
            projects: None,
            ai: None,
            memory: None,
            risk_acknowledged: false,
        };

        validate_approval_settings_update(&req, &state).expect("unchanged mode");
    }

    #[test]
    fn elevated_project_approval_modes_require_risk_acknowledgement() {
        assert!(!approval_projects_require_risk_ack(
            &[project(Some(ApprovalMode::RequestApproval,))],
            &[]
        ));
        assert!(approval_projects_require_risk_ack(
            &[project(Some(ApprovalMode::AutoApproval,))],
            &[]
        ));
        assert!(approval_projects_require_risk_ack(
            &[project(Some(ApprovalMode::FullControl,))],
            &[]
        ));
    }

    #[test]
    fn project_approval_risk_ack_is_only_required_for_actual_elevation() {
        assert!(!approval_projects_require_risk_ack(
            &[project(Some(ApprovalMode::AutoApproval))],
            &[project(Some(ApprovalMode::AutoApproval))],
        ));
        assert!(approval_projects_require_risk_ack(
            &[project(Some(ApprovalMode::FullControl))],
            &[project(Some(ApprovalMode::AutoApproval))],
        ));
        assert!(!approval_projects_require_risk_ack(
            &[project(Some(ApprovalMode::AutoApproval))],
            &[project(Some(ApprovalMode::FullControl))],
        ));
    }

    #[test]
    fn approval_settings_changed_ignores_revision_only_changes() {
        let current = crate::approval::ApprovalState::default();
        let mut next = current.clone();
        next.settings_revision = Some("local-revision".to_string());

        assert!(!approval_settings_changed(&current, &next));

        next.default_mode = ApprovalMode::AutoApproval;
        assert!(approval_settings_changed(&current, &next));
    }

    #[test]
    fn remember_allow_requires_risk_acknowledgement() {
        let req = ResolveApprovalRequest {
            remember_allow: Some(true),
            reason: None,
            risk_acknowledged: false,
        };
        assert!(req.remember_allow.unwrap_or(false) && !req.risk_acknowledged);

        let req = ResolveApprovalRequest {
            remember_allow: Some(true),
            reason: None,
            risk_acknowledged: true,
        };
        assert!(req.remember_allow.unwrap_or(false) && req.risk_acknowledged);
    }
}

pub(crate) async fn local_deny_pending_approval(
    Path(id): Path<String>,
    Json(req): Json<ResolveApprovalRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let ok = deny_pending_approval(id.as_str(), req.reason).await;
    if !ok {
        return Err(LocalApiError::bad_request("pending approval not found"));
    }
    Ok(Json(json!({ "ok": true })))
}

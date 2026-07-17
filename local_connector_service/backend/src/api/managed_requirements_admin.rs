// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use chatos_sandbox_contract::parse_managed_requirements_toml;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    now_rfc3339, CurrentUser, ManagedRequirementsAssignment, ManagedRequirementsPolicy,
    MANAGED_REQUIREMENTS_SCOPE_GLOBAL, MANAGED_REQUIREMENTS_SCOPE_ROLE,
    MANAGED_REQUIREMENTS_SCOPE_USER,
};
use crate::state::AppState;

use super::ApiError;

const MAX_REQUIREMENTS_BYTES: usize = 1024 * 1024;
const MAX_NAME_BYTES: usize = 200;
const MAX_DESCRIPTION_BYTES: usize = 4 * 1024;
const MAX_SUBJECT_BYTES: usize = 256;
const MAX_ASSIGNMENT_PRIORITY: i32 = 1_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateManagedRequirementsPolicyRequest {
    name: String,
    description: Option<String>,
    requirements_toml: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateManagedRequirementsPolicyRequest {
    name: Option<String>,
    description: Option<Option<String>>,
    requirements_toml: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateManagedRequirementsAssignmentRequest {
    policy_id: String,
    scope: String,
    subject: Option<String>,
    #[serde(default)]
    priority: i32,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateManagedRequirementsAssignmentRequest {
    policy_id: Option<String>,
    scope: Option<String>,
    subject: Option<Option<String>>,
    priority: Option<i32>,
    enabled: Option<bool>,
}

pub(super) async fn list_managed_requirements_policies(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<ManagedRequirementsPolicy>>, ApiError> {
    require_super_admin(&user)?;
    state
        .store
        .list_managed_requirements_policies()
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_managed_requirements_policy(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(request): Json<CreateManagedRequirementsPolicyRequest>,
) -> Result<(StatusCode, Json<ManagedRequirementsPolicy>), ApiError> {
    require_super_admin(&user)?;
    let requirements_toml = validate_requirements_toml(request.requirements_toml)?;
    let now = now_rfc3339();
    let policy = ManagedRequirementsPolicy {
        id: Uuid::new_v4().to_string(),
        name: required_limited_text(request.name, "name", MAX_NAME_BYTES)?,
        description: optional_limited_text(
            request.description,
            "description",
            MAX_DESCRIPTION_BYTES,
        )?,
        content_sha256: requirements_digest(requirements_toml.as_bytes()),
        requirements_toml,
        version: 1,
        enabled: request.enabled,
        created_by: user.user_id.clone(),
        updated_by: user.user_id,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .create_managed_requirements_policy(&policy)
        .await
        .map_err(management_store_error)?;
    Ok((StatusCode::CREATED, Json(policy)))
}

pub(super) async fn update_managed_requirements_policy(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(policy_id): Path<String>,
    Json(request): Json<UpdateManagedRequirementsPolicyRequest>,
) -> Result<Json<ManagedRequirementsPolicy>, ApiError> {
    require_super_admin(&user)?;
    let mut policy = state
        .store
        .get_managed_requirements_policy(policy_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("managed requirements policy not found"))?;
    if let Some(name) = request.name {
        policy.name = required_limited_text(name, "name", MAX_NAME_BYTES)?;
    }
    if let Some(description) = request.description {
        policy.description =
            optional_limited_text(description, "description", MAX_DESCRIPTION_BYTES)?;
    }
    if let Some(requirements_toml) = request.requirements_toml {
        policy.requirements_toml = validate_requirements_toml(requirements_toml)?;
        policy.content_sha256 = requirements_digest(policy.requirements_toml.as_bytes());
    }
    if let Some(enabled) = request.enabled {
        policy.enabled = enabled;
    }
    policy.version = policy.version.saturating_add(1);
    policy.updated_by = user.user_id;
    policy.updated_at = now_rfc3339();
    if !state
        .store
        .update_managed_requirements_policy(&policy)
        .await
        .map_err(management_store_error)?
    {
        return Err(ApiError::not_found("managed requirements policy not found"));
    }
    Ok(Json(policy))
}

pub(super) async fn delete_managed_requirements_policy(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(policy_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_super_admin(&user)?;
    if state
        .store
        .managed_requirements_policy_has_assignments(policy_id.as_str())
        .await
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::conflict(
            "managed_requirements_policy_in_use",
            "managed requirements policy still has assignments",
        ));
    }
    if !state
        .store
        .delete_managed_requirements_policy(policy_id.as_str())
        .await
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::not_found("managed requirements policy not found"));
    }
    Ok(Json(json!({ "success": true })))
}

pub(super) async fn list_managed_requirements_assignments(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<ManagedRequirementsAssignment>>, ApiError> {
    require_super_admin(&user)?;
    state
        .store
        .list_managed_requirements_assignments()
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_managed_requirements_assignment(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(request): Json<CreateManagedRequirementsAssignmentRequest>,
) -> Result<(StatusCode, Json<ManagedRequirementsAssignment>), ApiError> {
    require_super_admin(&user)?;
    let policy_id = required_limited_text(request.policy_id, "policy_id", MAX_SUBJECT_BYTES)?;
    ensure_policy_exists(&state, policy_id.as_str()).await?;
    let (scope, subject) = normalize_assignment_scope(request.scope, request.subject)?;
    let now = now_rfc3339();
    let assignment = ManagedRequirementsAssignment {
        id: Uuid::new_v4().to_string(),
        policy_id,
        scope,
        subject,
        priority: validate_assignment_priority(request.priority)?,
        enabled: request.enabled,
        created_by: user.user_id.clone(),
        updated_by: user.user_id,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .create_managed_requirements_assignment(&assignment)
        .await
        .map_err(management_store_error)?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

pub(super) async fn update_managed_requirements_assignment(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(assignment_id): Path<String>,
    Json(request): Json<UpdateManagedRequirementsAssignmentRequest>,
) -> Result<Json<ManagedRequirementsAssignment>, ApiError> {
    require_super_admin(&user)?;
    let mut assignment = state
        .store
        .get_managed_requirements_assignment(assignment_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("managed requirements assignment not found"))?;
    if let Some(policy_id) = request.policy_id {
        let policy_id = required_limited_text(policy_id, "policy_id", MAX_SUBJECT_BYTES)?;
        ensure_policy_exists(&state, policy_id.as_str()).await?;
        assignment.policy_id = policy_id;
    }
    if request.scope.is_some() || request.subject.is_some() {
        let scope = request.scope.unwrap_or_else(|| assignment.scope.clone());
        let subject = request
            .subject
            .unwrap_or_else(|| assignment.subject.clone());
        (assignment.scope, assignment.subject) = normalize_assignment_scope(scope, subject)?;
    }
    if let Some(priority) = request.priority {
        assignment.priority = validate_assignment_priority(priority)?;
    }
    if let Some(enabled) = request.enabled {
        assignment.enabled = enabled;
    }
    assignment.updated_by = user.user_id;
    assignment.updated_at = now_rfc3339();
    if !state
        .store
        .update_managed_requirements_assignment(&assignment)
        .await
        .map_err(management_store_error)?
    {
        return Err(ApiError::not_found(
            "managed requirements assignment not found",
        ));
    }
    Ok(Json(assignment))
}

pub(super) async fn delete_managed_requirements_assignment(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(assignment_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_super_admin(&user)?;
    if !state
        .store
        .delete_managed_requirements_assignment(assignment_id.as_str())
        .await
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::not_found(
            "managed requirements assignment not found",
        ));
    }
    Ok(Json(json!({ "success": true })))
}

fn require_super_admin(user: &CurrentUser) -> Result<(), ApiError> {
    if user.is_super_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "managed requirements administration requires a human super administrator",
        ))
    }
}

async fn ensure_policy_exists(state: &AppState, policy_id: &str) -> Result<(), ApiError> {
    if state
        .store
        .get_managed_requirements_policy(policy_id)
        .await
        .map_err(ApiError::internal)?
        .is_some()
    {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "managed requirements policy_id does not exist",
        ))
    }
}

fn validate_requirements_toml(value: String) -> Result<String, ApiError> {
    if value.len() > MAX_REQUIREMENTS_BYTES {
        return Err(ApiError::bad_request(
            "managed requirements TOML exceeds the 1 MiB limit",
        ));
    }
    parse_managed_requirements_toml(value.as_str()).map_err(ApiError::bad_request)?;
    Ok(value)
}

fn normalize_assignment_scope(
    scope: String,
    subject: Option<String>,
) -> Result<(String, Option<String>), ApiError> {
    let scope = scope.trim().to_ascii_lowercase();
    match scope.as_str() {
        MANAGED_REQUIREMENTS_SCOPE_GLOBAL => {
            if subject
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(ApiError::bad_request(
                    "global managed requirements assignments must not define subject",
                ));
            }
            Ok((scope, None))
        }
        MANAGED_REQUIREMENTS_SCOPE_ROLE | MANAGED_REQUIREMENTS_SCOPE_USER => Ok((
            scope,
            Some(required_limited_text(
                subject.unwrap_or_default(),
                "subject",
                MAX_SUBJECT_BYTES,
            )?),
        )),
        _ => Err(ApiError::bad_request(
            "managed requirements assignment scope must be global, role, or user",
        )),
    }
}

fn validate_assignment_priority(priority: i32) -> Result<i32, ApiError> {
    if (-MAX_ASSIGNMENT_PRIORITY..=MAX_ASSIGNMENT_PRIORITY).contains(&priority) {
        Ok(priority)
    } else {
        Err(ApiError::bad_request(
            "managed requirements assignment priority must be between -1000 and 1000",
        ))
    }
}

fn required_limited_text(value: String, label: &str, max_bytes: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request(format!("{label} is required")));
    }
    if value.len() > max_bytes {
        return Err(ApiError::bad_request(format!(
            "{label} exceeds the size limit"
        )));
    }
    Ok(value.to_string())
}

fn optional_limited_text(
    value: Option<String>,
    label: &str,
    max_bytes: usize,
) -> Result<Option<String>, ApiError> {
    value
        .map(|value| required_limited_text(value, label, max_bytes))
        .transpose()
}

fn requirements_digest(value: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value)))
}

fn management_store_error(error: String) -> ApiError {
    if error.contains("E11000") || error.contains("duplicate key") {
        ApiError::conflict(
            "managed_requirements_duplicate",
            "managed requirements policy or assignment already exists",
        )
    } else {
        ApiError::internal(error)
    }
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role: &str, principal_type: &str) -> CurrentUser {
        CurrentUser {
            principal_type: principal_type.to_string(),
            user_id: "admin-1".to_string(),
            username: Some("admin".to_string()),
            display_name: Some("Admin".to_string()),
            role: role.to_string(),
            owner_user_id: None,
        }
    }

    #[test]
    fn only_human_super_admin_can_manage_requirements() {
        assert!(require_super_admin(&user("super_admin", "human_user")).is_ok());
        assert!(require_super_admin(&user("user", "human_user")).is_err());
        assert!(require_super_admin(&user("super_admin", "agent_account")).is_err());
    }

    #[test]
    fn assignment_scope_requires_correct_subject_shape() {
        assert_eq!(
            normalize_assignment_scope("global".to_string(), None).unwrap(),
            ("global".to_string(), None)
        );
        assert!(
            normalize_assignment_scope("global".to_string(), Some("user-1".to_string())).is_err()
        );
        assert_eq!(
            normalize_assignment_scope("user".to_string(), Some(" user-1 ".to_string())).unwrap(),
            ("user".to_string(), Some("user-1".to_string()))
        );
        assert!(normalize_assignment_scope("role".to_string(), None).is_err());
    }

    #[test]
    fn policy_toml_is_strictly_parsed_before_storage() {
        assert!(
            validate_requirements_toml("default_permissions = \":read-only\"".to_string()).is_ok()
        );
        assert!(validate_requirements_toml("unsupported = true".to_string()).is_err());
    }
}

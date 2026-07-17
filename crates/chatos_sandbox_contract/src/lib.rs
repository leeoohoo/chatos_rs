// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

mod filesystem;
mod managed_requirements;
mod permissions;
mod profiles;
mod toml_profiles;

pub use filesystem::*;
pub use managed_requirements::*;
pub use permissions::*;
pub use profiles::*;
pub use toml_profiles::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxBackendKind {
    LocalProcess,
    #[default]
    Docker,
}

impl SandboxBackendKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalProcess => "local_process",
            Self::Docker => "docker",
        }
    }
}

impl std::str::FromStr for SandboxBackendKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "local_process" | "process" => Ok(Self::LocalProcess),
            "docker" => Ok(Self::Docker),
            other => Err(format!("unsupported sandbox backend: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionProfileId {
    ReadOnly,
    #[default]
    WorkspaceWrite,
    FullAccess,
}

impl PermissionProfileId {
    pub const ALL: [Self; 3] = [Self::ReadOnly, Self::WorkspaceWrite, Self::FullAccess];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::WorkspaceWrite => "workspace_write",
            Self::FullAccess => "full_access",
        }
    }

    pub const fn codex_name(self) -> &'static str {
        match self {
            Self::ReadOnly => ":read-only",
            Self::WorkspaceWrite => ":workspace",
            Self::FullAccess => ":danger-full-access",
        }
    }

    pub const fn rank(self) -> u8 {
        match self {
            Self::ReadOnly => 0,
            Self::WorkspaceWrite => 1,
            Self::FullAccess => 2,
        }
    }

    pub const fn is_no_broader_than(self, maximum: Self) -> bool {
        self.rank() <= maximum.rank()
    }
}

impl std::str::FromStr for PermissionProfileId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read_only" | "read-only" | ":read-only" => Ok(Self::ReadOnly),
            "workspace_write" | "workspace-write" | ":workspace" => Ok(Self::WorkspaceWrite),
            "full_access" | "danger-full-access" | ":danger-full-access" => Ok(Self::FullAccess),
            other => Err(format!("unsupported permission profile: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    #[default]
    OnRequest,
    Never,
}

impl ApprovalPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnRequest => "on_request",
            Self::Never => "never",
        }
    }

    pub const fn rank(self) -> u8 {
        match self {
            // `never` cannot grant an escalation: requests outside the sandbox fail closed.
            Self::Never => 0,
            Self::OnRequest => 1,
        }
    }

    pub const fn is_no_broader_than(self, maximum: Self) -> bool {
        self.rank() <= maximum.rank()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalReviewer {
    #[default]
    User,
    AutoReview,
}

impl ApprovalReviewer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::AutoReview => "auto_review",
        }
    }

    pub const fn rank(self) -> u8 {
        match self {
            Self::User => 0,
            Self::AutoReview => 1,
        }
    }

    pub const fn is_no_broader_than(self, maximum: Self) -> bool {
        self.rank() <= maximum.rank()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxLeasePolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_mode: Option<SandboxBackendKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_profile_id: Option<PermissionProfileId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<ApprovalPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_reviewer: Option<ApprovalReviewer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_writable_roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveSandboxPolicy {
    pub sandbox_mode: SandboxBackendKind,
    pub permission_profile_id: PermissionProfileId,
    pub approval_policy: ApprovalPolicy,
    pub approval_reviewer: ApprovalReviewer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_writable_roots: Vec<String>,
}

impl Default for EffectiveSandboxPolicy {
    fn default() -> Self {
        Self {
            sandbox_mode: SandboxBackendKind::Docker,
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            approval_policy: ApprovalPolicy::OnRequest,
            approval_reviewer: ApprovalReviewer::User,
            policy_revision: None,
            additional_writable_roots: Vec::new(),
        }
    }
}

impl EffectiveSandboxPolicy {
    pub fn resolve(request: &SandboxLeasePolicyRequest, defaults: &Self) -> Self {
        Self {
            sandbox_mode: request.sandbox_mode.unwrap_or(defaults.sandbox_mode),
            permission_profile_id: request
                .permission_profile_id
                .unwrap_or(defaults.permission_profile_id),
            approval_policy: request.approval_policy.unwrap_or(defaults.approval_policy),
            approval_reviewer: request
                .approval_reviewer
                .unwrap_or(defaults.approval_reviewer),
            policy_revision: request
                .policy_revision
                .clone()
                .or_else(|| defaults.policy_revision.clone()),
            additional_writable_roots: request.additional_writable_roots.clone(),
        }
    }

    pub fn resolve_no_broader_than(request: &SandboxLeasePolicyRequest, maximum: &Self) -> Self {
        let requested_permission = request
            .permission_profile_id
            .unwrap_or(maximum.permission_profile_id);
        let requested_approval_policy = request.approval_policy.unwrap_or(maximum.approval_policy);
        let requested_approval_reviewer = request
            .approval_reviewer
            .unwrap_or(maximum.approval_reviewer);
        Self {
            // Backend is not a permission rank. A requested unsupported backend must remain visible
            // to the caller so readiness checks can fail closed instead of silently substituting.
            sandbox_mode: request.sandbox_mode.unwrap_or(maximum.sandbox_mode),
            permission_profile_id: if requested_permission
                .is_no_broader_than(maximum.permission_profile_id)
            {
                requested_permission
            } else {
                maximum.permission_profile_id
            },
            approval_policy: if requested_approval_policy
                .is_no_broader_than(maximum.approval_policy)
            {
                requested_approval_policy
            } else {
                maximum.approval_policy
            },
            approval_reviewer: if requested_approval_reviewer
                .is_no_broader_than(maximum.approval_reviewer)
            {
                requested_approval_reviewer
            } else {
                maximum.approval_reviewer
            },
            policy_revision: maximum.policy_revision.clone(),
            additional_writable_roots: requested_additional_writable_roots_within_maximum(
                request.additional_writable_roots.as_slice(),
                maximum.additional_writable_roots.as_slice(),
            ),
        }
    }
}

fn requested_additional_writable_roots_within_maximum(
    requested: &[String],
    maximum: &[String],
) -> Vec<String> {
    requested
        .iter()
        .filter_map(|value| normalized_root(value))
        .filter(|requested_root| {
            maximum
                .iter()
                .filter_map(|value| normalized_root(value))
                .any(|maximum_root| maximum_root == *requested_root)
        })
        .collect()
}

fn normalized_root(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxBackendReadinessStatus {
    Ready,
    SetupRequired,
    Unsupported,
    UnderDevelopment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxBackendCapability {
    pub backend: SandboxBackendKind,
    pub status: SandboxBackendReadinessStatus,
    pub selectable: bool,
    pub filesystem_isolation: bool,
    pub network_isolation: bool,
    pub process_tree_control: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_empty_policy_resolves_to_compatible_docker_defaults() {
        let request: SandboxLeasePolicyRequest =
            serde_json::from_value(serde_json::json!({})).expect("policy");
        let effective =
            EffectiveSandboxPolicy::resolve(&request, &EffectiveSandboxPolicy::default());

        assert_eq!(effective.sandbox_mode, SandboxBackendKind::Docker);
        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::OnRequest);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
    }

    #[test]
    fn local_process_policy_deserializes_and_resolves_explicit_fields() {
        let request: SandboxLeasePolicyRequest = serde_json::from_value(serde_json::json!({
            "sandbox_mode": "local_process",
            "permission_profile_id": "full_access",
            "approval_policy": "never",
            "approval_reviewer": "auto_review",
            "policy_revision": "revision-2",
            "additional_writable_roots": ["C:/tmp"]
        }))
        .expect("policy");
        let effective =
            EffectiveSandboxPolicy::resolve(&request, &EffectiveSandboxPolicy::default());

        assert_eq!(effective.sandbox_mode, SandboxBackendKind::LocalProcess);
        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::FullAccess
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::Never);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::AutoReview);
        assert_eq!(effective.policy_revision.as_deref(), Some("revision-2"));
        assert_eq!(effective.additional_writable_roots, vec!["C:/tmp"]);
    }

    #[test]
    fn permission_profiles_have_a_stable_restrictiveness_order() {
        assert!(
            PermissionProfileId::ReadOnly.is_no_broader_than(PermissionProfileId::WorkspaceWrite)
        );
        assert!(
            PermissionProfileId::WorkspaceWrite.is_no_broader_than(PermissionProfileId::FullAccess)
        );
        assert!(!PermissionProfileId::FullAccess
            .is_no_broader_than(PermissionProfileId::WorkspaceWrite));
        assert_eq!(PermissionProfileId::ReadOnly.codex_name(), ":read-only");
        assert_eq!(
            PermissionProfileId::WorkspaceWrite.codex_name(),
            ":workspace"
        );
        assert_eq!(
            PermissionProfileId::FullAccess.codex_name(),
            ":danger-full-access"
        );
    }

    #[test]
    fn approval_policy_and_reviewer_have_a_stable_restrictiveness_order() {
        assert!(ApprovalPolicy::Never.is_no_broader_than(ApprovalPolicy::OnRequest));
        assert!(!ApprovalPolicy::OnRequest.is_no_broader_than(ApprovalPolicy::Never));
        assert!(ApprovalReviewer::User.is_no_broader_than(ApprovalReviewer::AutoReview));
        assert!(!ApprovalReviewer::AutoReview.is_no_broader_than(ApprovalReviewer::User));
    }

    #[test]
    fn capped_policy_never_exceeds_local_maximum() {
        let request = SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::FullAccess),
            approval_policy: Some(ApprovalPolicy::Never),
            approval_reviewer: Some(ApprovalReviewer::AutoReview),
            policy_revision: Some("request-revision".to_string()),
            additional_writable_roots: vec![
                "C:/allowed".to_string(),
                "C:/outside".to_string(),
                " ".to_string(),
            ],
        };
        let maximum = EffectiveSandboxPolicy {
            sandbox_mode: SandboxBackendKind::Docker,
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            approval_policy: ApprovalPolicy::OnRequest,
            approval_reviewer: ApprovalReviewer::User,
            policy_revision: Some("local-revision".to_string()),
            additional_writable_roots: vec![" C:/allowed ".to_string()],
        };

        let effective = EffectiveSandboxPolicy::resolve_no_broader_than(&request, &maximum);

        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::Never);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
        assert_eq!(effective.policy_revision.as_deref(), Some("local-revision"));
        assert_eq!(effective.additional_writable_roots, vec!["C:/allowed"]);
    }

    #[test]
    fn capped_policy_allows_narrower_fields_without_reenabling_approvals() {
        let request = SandboxLeasePolicyRequest {
            sandbox_mode: None,
            permission_profile_id: Some(PermissionProfileId::ReadOnly),
            approval_policy: Some(ApprovalPolicy::OnRequest),
            approval_reviewer: Some(ApprovalReviewer::User),
            policy_revision: None,
            additional_writable_roots: Vec::new(),
        };
        let maximum = EffectiveSandboxPolicy {
            sandbox_mode: SandboxBackendKind::Docker,
            permission_profile_id: PermissionProfileId::FullAccess,
            approval_policy: ApprovalPolicy::Never,
            approval_reviewer: ApprovalReviewer::AutoReview,
            policy_revision: Some("local-revision".to_string()),
            additional_writable_roots: Vec::new(),
        };

        let effective = EffectiveSandboxPolicy::resolve_no_broader_than(&request, &maximum);

        assert_eq!(effective.sandbox_mode, SandboxBackendKind::Docker);
        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::ReadOnly
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::Never);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
    }

    #[test]
    fn capped_policy_preserves_requested_backend_for_fail_closed_readiness() {
        let request = SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::LocalProcess),
            ..SandboxLeasePolicyRequest::default()
        };

        let effective = EffectiveSandboxPolicy::resolve_no_broader_than(
            &request,
            &EffectiveSandboxPolicy::default(),
        );

        assert_eq!(effective.sandbox_mode, SandboxBackendKind::LocalProcess);
    }
}

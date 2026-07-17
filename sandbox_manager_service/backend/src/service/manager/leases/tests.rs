// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn requested_network_mode_allows_empty_bridge_or_configured_default() {
    assert!(requested_network_mode_is_allowed("", None));
    assert!(requested_network_mode_is_allowed(" bridge ", None));
    assert!(requested_network_mode_is_allowed(
        "sandbox-internal",
        Some("sandbox-internal")
    ));
    assert!(requested_network_mode_is_allowed(
        "SANDBOX-INTERNAL",
        Some("sandbox-internal")
    ));
}

#[test]
fn requested_network_mode_rejects_boundary_expanding_overrides() {
    assert!(!requested_network_mode_is_allowed("host", Some("bridge")));
    assert!(!requested_network_mode_is_allowed(
        "container:other",
        Some("bridge")
    ));
    assert!(!requested_network_mode_is_allowed(
        "prod-db-network",
        Some("sandbox-internal")
    ));
    assert!(!requested_network_mode_is_allowed("none", Some("bridge")));
}

#[test]
fn effective_policy_reports_only_capabilities_enforced_by_manager() {
    let request = SandboxLeasePolicyRequest {
        sandbox_mode: Some(SandboxBackendKind::Docker),
        permission_profile_id: Some(PermissionProfileId::ReadOnly),
        approval_policy: Some(ApprovalPolicy::OnRequest),
        approval_reviewer: Some(ApprovalReviewer::AutoReview),
        policy_revision: Some("request-revision".to_string()),
        additional_writable_roots: vec!["/outside".to_string()],
    };

    let effective = sandbox_manager_effective_policy(&request);

    assert_eq!(effective.sandbox_mode, SandboxBackendKind::Docker);
    assert_eq!(
        effective.permission_profile_id,
        PermissionProfileId::WorkspaceWrite
    );
    assert_eq!(effective.approval_policy, ApprovalPolicy::Never);
    assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
    assert_eq!(
        effective.policy_revision.as_deref(),
        Some("request-revision")
    );
    assert!(effective.additional_writable_roots.is_empty());
}

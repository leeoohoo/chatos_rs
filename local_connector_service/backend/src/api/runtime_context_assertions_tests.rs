// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use memory_engine_sdk::{
    verify_runtime_context_assertion, ExpectedRuntimeContext, RuntimeContextAssertionClaims,
    RuntimeContextAssertionIssuer, MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
};

const SECRET: &str = "local-project-runtime-assertion-test-secret";

fn issuer() -> RuntimeContextAssertionIssuer {
    RuntimeContextAssertionIssuer::new(SECRET, "test-key", "connector", "test-revision", 60)
        .unwrap()
}

fn user() -> CurrentUser {
    CurrentUser {
        principal_type: "human_user".to_string(),
        token_jti: None,
        user_id: "owner-a".to_string(),
        username: None,
        display_name: None,
        role: "user".to_string(),
        owner_user_id: None,
        scopes: vec![],
    }
}

fn request(project_id: Option<&str>, operation: &str) -> IssueRuntimeContextAssertionRequest {
    IssueRuntimeContextAssertionRequest {
        runtime_kind: "local_agent".to_string(),
        project_id: project_id.map(str::to_string),
        runtime_id: "client-agent:thread-a".to_string(),
        operation: operation.to_string(),
        source_id: Some("chatos".to_string()),
        canonical_agent_subject_id: Some("agent:main-chat".to_string()),
        thread_id: Some("client-agent:thread-a".to_string()),
    }
}

fn verify(
    response: &IssueRuntimeContextAssertionResponse,
    tenant: &str,
    partition: &str,
    operation: &str,
) -> Result<RuntimeContextAssertionClaims, String> {
    verify_runtime_context_assertion(
        &response.runtime_context_assertion,
        SECRET,
        "connector",
        ExpectedRuntimeContext {
            audience: MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
            tenant_id: tenant,
            source_id: &response.source_id,
            thread_id: &response.thread_id,
            project_partition_key: partition,
            operation,
        },
    )
}

#[test]
fn local_projects_issue_all_operations_in_authenticated_owner_scope() {
    for project in [Some("local-project-a"), None] {
        for operation in ["compose", "record_ingest", "summarize", "repair"] {
            let Json(response) = issue_for_runtime(&issuer(), &user(), request(project, operation))
                .expect("issue local project assertion");
            let partition = project
                .map(|id| format!("project:{id}"))
                .unwrap_or_else(|| "__none__".to_string());
            let claims = verify(&response, "owner-a", &partition, operation).unwrap();
            assert_eq!(claims.principal_id, "owner-a");
            assert_eq!(claims.canonical_agent_subject_id, "agent:main-chat");
            assert!(verify(&response, "owner-b", &partition, operation).is_err());
            assert!(verify(&response, "owner-a", "project:other", operation).is_err());
            let other_operation = if operation == "compose" {
                "repair"
            } else {
                "compose"
            };
            assert!(verify(&response, "owner-a", &partition, other_operation).is_err());
        }
    }
}

#[test]
fn delegated_local_runtime_uses_authenticated_owner_namespace() {
    let mut caller = user();
    caller.user_id = "device-principal".to_string();
    caller.owner_user_id = Some("owner-a".to_string());
    let Json(response) = issue_for_runtime(
        &issuer(),
        &caller,
        request(Some("local-project-a"), "compose"),
    )
    .unwrap();
    let claims = verify(&response, "owner-a", "project:local-project-a", "compose").unwrap();
    assert_eq!(claims.principal_id, "owner-a");
    assert!(verify(
        &response,
        "device-principal",
        "project:local-project-a",
        "compose"
    )
    .is_err());
}

#[test]
fn story_studio_project_identity_is_owner_scoped_without_cloud_project_records() {
    for operation in ["compose", "record_ingest", "summarize", "repair"] {
        let mut req = request(Some("story-project"), operation);
        req.runtime_kind = "story_studio".to_string();
        let Json(response) = issue_for_runtime(&issuer(), &user(), req).unwrap();
        let claims = verify(&response, "owner-a", "project:story-project", operation).unwrap();
        assert_eq!(
            claims.canonical_agent_subject_id,
            "agent:story-studio:story-project"
        );
        assert!(verify(&response, "owner-b", "project:story-project", operation).is_err());
        assert!(verify(&response, "owner-a", "project:other", operation).is_err());
    }
}

#[test]
fn invalid_runtime_identity_is_rejected() {
    let mut cases = vec![];
    let mut req = request(Some("local-project-a"), "compose");
    req.runtime_kind = "unknown".to_string();
    cases.push(req);
    let mut req = request(Some("local-project-a"), "compose");
    req.runtime_id = "different-thread".to_string();
    cases.push(req);
    let mut req = request(Some("local-project-a"), "compose");
    req.canonical_agent_subject_id = Some("user:forged".to_string());
    cases.push(req);
    cases.push(request(Some("local-project-a"), "unknown-operation"));
    cases.push(request(Some(" \n "), "compose"));
    let mut req = request(None, "compose");
    req.runtime_kind = "story_studio".to_string();
    cases.push(req);

    for req in cases {
        let error = issue_for_runtime(&issuer(), &user(), req).unwrap_err();
        assert_eq!(
            axum::response::IntoResponse::into_response(error).status(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }
}

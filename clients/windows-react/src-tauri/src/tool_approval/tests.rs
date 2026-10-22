use chatos_local_agent_protocol::{
    LocalAgentToolApprovalStatus, LocalAgentToolExecutionRoute, LocalAgentToolInvocationRecord,
    LocalAgentToolStatus,
};
use serde_json::json;

use super::{
    models::{ToolApprovalDecision, ToolApprovalMode},
    policy_auto_approval_candidate, project_pending,
};

#[test]
fn projects_only_safe_fields_and_never_serializes_full_arguments() {
    let approval = invocation(
        "approval-1",
        "execute_command",
        json!({
            "command": "cargo test",
            "purpose": "验证当前改动",
            "path": "clients/windows-react",
            "api_key": "must-never-cross-the-ui-boundary",
            "nested": { "password": "also-hidden" }
        }),
    );
    let projected = project_pending(vec![approval]);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].command, "cargo test");
    assert_eq!(projected[0].purpose.as_deref(), Some("验证当前改动"));
    assert_eq!(projected[0].cwd, "clients/windows-react");
    assert_eq!(
        projected[0].available_decisions,
        vec![
            ToolApprovalDecision::Decline,
            ToolApprovalDecision::AcceptForSession,
            ToolApprovalDecision::Accept,
        ]
    );
    let encoded = serde_json::to_string(&projected).expect("projection should serialize");
    assert!(!encoded.contains("must-never"));
    assert!(!encoded.contains("also-hidden"));
    assert!(!encoded.contains("api_key"));
    assert!(!encoded.contains("arguments"));
}

#[test]
fn projects_the_complete_executable_command_without_truncation() {
    let command = format!(
        "{}printf 'tail marker' | sed -n '1p'",
        "printf x; ".repeat(500)
    );
    let projected = project_pending(vec![invocation(
        "approval-1",
        "remote_connection_controller_run_command",
        json!({"command": command}),
    )]);

    assert!(command.chars().count() > 4_000);
    assert_eq!(
        projected[0].command,
        format!("remote SSH command · {command}")
    );
    assert!(projected[0]
        .command
        .ends_with("printf 'tail marker' | sed -n '1p'"));
}

#[test]
fn publishes_session_decision_only_for_supported_terminal_changes() {
    let projected = project_pending(vec![
        invocation("execute", "execute_command", json!({})),
        invocation("write", "process_write", json!({})),
        invocation("kill", "process_kill", json!({})),
        invocation("other", "commit_edit_session", json!({})),
    ]);
    for approval in projected
        .iter()
        .filter(|approval| approval.invocation_id != "other")
    {
        assert!(approval
            .available_decisions
            .contains(&ToolApprovalDecision::AcceptForSession));
    }
    assert!(!projected
        .iter()
        .find(|approval| approval.invocation_id == "other")
        .unwrap()
        .available_decisions
        .contains(&ToolApprovalDecision::AcceptForSession));
    assert_eq!(
        serde_json::to_string(&ToolApprovalDecision::AcceptForSession).unwrap(),
        "\"acceptForSession\""
    );
}

#[test]
fn presents_plugin_invocation_as_a_single_high_risk_decision_without_arguments() {
    let projected = project_pending(vec![invocation(
        "plugin",
        "capability_invoke",
        json!({
            "plugin_option": "plugin-secret-token",
            "tool_option": "tool-secret-token",
            "arguments": {
                "document": "must-not-cross-the-approval-boundary",
                "api_key": "also-hidden"
            }
        }),
    )]);

    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].risk, "high");
    assert_eq!(
        projected[0].available_decisions,
        [ToolApprovalDecision::Decline, ToolApprovalDecision::Accept]
    );
    assert!(projected[0].command.contains("selected Plugin tool"));
    assert!(!projected[0].command.contains("staged project edits"));
    let encoded = serde_json::to_string(&projected).unwrap();
    assert!(!encoded.contains("must-not-cross"));
    assert!(!encoded.contains("also-hidden"));
    assert!(!encoded.contains("plugin-secret-token"));
    assert!(!encoded.contains("tool-secret-token"));
}

#[test]
fn mirrors_macos_terminal_risk_markers() {
    let projected = project_pending(vec![invocation(
        "approval-1",
        "execute_command",
        json!({"command": "git reset --hard HEAD", "purpose": "清理工作区"}),
    )]);
    assert_eq!(projected[0].risk, "high");
    assert!(projected[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("git reset --hard")));
}

#[test]
fn hides_process_input_and_rejects_non_pending_records() {
    let mut hidden = invocation(
        "approval-1",
        "process_write",
        json!({"input": "secret interactive input"}),
    );
    let mut resolved = invocation("approval-2", "execute_command", json!({}));
    resolved.approval_status = LocalAgentToolApprovalStatus::Approved;
    let mut running = invocation("approval-3", "execute_command", json!({}));
    running.status = LocalAgentToolStatus::Running;
    let projected = project_pending(vec![resolved, hidden.clone(), running]);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].invocation_id, "approval-1");
    assert!(projected[0].command.contains("input hidden"));
    assert!(!serde_json::to_string(&projected)
        .expect("projection should serialize")
        .contains("secret interactive input"));

    hidden.requires_approval = false;
    assert!(project_pending(vec![hidden]).is_empty());
}

#[test]
fn sorts_queue_by_creation_time_then_identity() {
    let mut later = invocation("z", "commit_edit_session", json!({"patch": "hidden"}));
    later.created_at_unix_ms = 20;
    let mut earlier_b = invocation("b", "commit_edit_session", json!({}));
    earlier_b.created_at_unix_ms = 10;
    let mut earlier_a = invocation("a", "commit_edit_session", json!({}));
    earlier_a.created_at_unix_ms = 10;
    assert_eq!(
        project_pending(vec![later, earlier_b, earlier_a])
            .into_iter()
            .map(|approval| approval.invocation_id)
            .collect::<Vec<_>>(),
        ["a", "b", "z"]
    );
}

#[test]
fn full_control_never_auto_approves_unqualified_or_non_terminal_tools() {
    let terminal = invocation("terminal", "execute_command", json!({}));
    let edit = invocation("edit", "commit_edit_session", json!({}));
    assert!(policy_auto_approval_candidate(
        ToolApprovalMode::FullControl,
        &terminal,
    ));
    assert!(!policy_auto_approval_candidate(
        ToolApprovalMode::AutoApproval,
        &terminal,
    ));
    assert!(!policy_auto_approval_candidate(
        ToolApprovalMode::FullControl,
        &edit,
    ));
    let mut already_approved = terminal;
    already_approved.approval_status = LocalAgentToolApprovalStatus::Approved;
    assert!(!policy_auto_approval_candidate(
        ToolApprovalMode::FullControl,
        &already_approved,
    ));
}

fn invocation(
    invocation_id: &str,
    tool_name: &str,
    arguments: serde_json::Value,
) -> LocalAgentToolInvocationRecord {
    LocalAgentToolInvocationRecord {
        invocation_id: invocation_id.into(),
        run_id: "run-1".into(),
        batch_id: "batch-1".into(),
        call_id: "call-1".into(),
        tool_name: tool_name.into(),
        execution_route: LocalAgentToolExecutionRoute::Platform,
        arguments,
        side_effecting: true,
        requires_approval: true,
        approval_status: LocalAgentToolApprovalStatus::Pending,
        approval_decided_by: None,
        approval_reason: None,
        approval_decided_at_unix_ms: None,
        status: LocalAgentToolStatus::Pending,
        result: None,
        error: None,
        version: 2,
        claim_token: Some("must-not-cross".into()),
        claim_until_unix_ms: None,
        created_at_unix_ms: 10,
        updated_at_unix_ms: 11,
    }
}

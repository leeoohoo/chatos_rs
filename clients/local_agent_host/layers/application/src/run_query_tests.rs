// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CreateRunCommand, HostCommand, HostRequestEnvelope, HostResult, ListRunsCommand,
    LocalAgentRunListScope, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::sync::Arc;

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn create(run_id: &str, owner_user_id: &str) -> HostCommand {
    HostCommand::CreateRun(CreateRunCommand {
        run_id: run_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: format!("conversation-{run_id}"),
        profile_key: "main_chat".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({"message": run_id}),
        max_iterations: 4,
    })
}

#[tokio::test]
async fn lists_only_owner_runs_with_a_stable_composite_cursor() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    for (run_id, owner) in [
        ("run-a", "user-1"),
        ("run-b", "user-1"),
        ("run-c", "user-1"),
        ("run-other", "user-2"),
    ] {
        runtime
            .try_handle(request(&format!("create-{run_id}"), create(run_id, owner)))
            .await
            .expect("create");
    }
    let first = runtime
        .try_handle(request(
            "list-first",
            HostCommand::ListRuns(ListRunsCommand {
                owner_user_id: "user-1".to_string(),
                scope: LocalAgentRunListScope::Active,
                before_updated_at_unix_ms: None,
                before_run_id: None,
                limit: 2,
            }),
        ))
        .await
        .expect("first page");
    let HostResult::Runs { page: first } = first else {
        panic!("unexpected first result")
    };
    assert_eq!(
        first
            .runs
            .iter()
            .map(|run| run.run_id.as_str())
            .collect::<Vec<_>>(),
        vec!["run-c", "run-b"]
    );
    assert_eq!(first.next_before_updated_at_unix_ms, Some(10_000));
    assert_eq!(first.next_before_run_id.as_deref(), Some("run-b"));

    let second = runtime
        .try_handle(request(
            "list-second",
            HostCommand::ListRuns(ListRunsCommand {
                owner_user_id: "user-1".to_string(),
                scope: LocalAgentRunListScope::All,
                before_updated_at_unix_ms: first.next_before_updated_at_unix_ms,
                before_run_id: first.next_before_run_id,
                limit: 2,
            }),
        ))
        .await
        .expect("second page");
    let HostResult::Runs { page: second } = second else {
        panic!("unexpected second result")
    };
    assert_eq!(second.runs.len(), 1);
    assert_eq!(second.runs[0].run_id, "run-a");
    assert_eq!(second.next_before_run_id, None);
}

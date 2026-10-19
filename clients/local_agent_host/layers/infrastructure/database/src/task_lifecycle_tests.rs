// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{
    IdempotentCommand, LocalAgentRunStore, LocalAgentTaskStore, LocalConversationStore,
    RunTransition, SqliteClientStorage,
};
use chatos_local_agent_protocol::{
    CreateConversationCommand, CreateTaskGraphCommand, LocalConversationResourceBinding,
    LocalConversationResourceKind, LocalTaskDependency, LocalTaskGraphStatus, LocalTaskSpec,
    LocalTaskStatus, StartConversationTurnCommand,
};
use serde_json::json;

#[test]
fn prerequisite_result_uses_one_hundred_sixty_thousand_character_inline_limit() {
    let nineteen_thousand = "解".repeat(19_961);
    assert_eq!(
        bounded_visible_text(&nineteen_thousand, Some("run-source")).as_deref(),
        Some(nineteen_thousand.as_str())
    );

    let oversized = "x".repeat(INLINE_PREREQUISITE_RESULT_MAX_CHARS + 1);
    let descriptor = bounded_visible_text(&oversized, Some("run-source")).expect("reference");
    let descriptor: serde_json::Value = serde_json::from_str(&descriptor).expect("descriptor");
    assert_eq!(descriptor["type"], "task_result_reference");
    assert_eq!(descriptor["source_run_id"], "run-source");
    assert_eq!(descriptor["read_tool"], "tool_result_read");
    assert_eq!(
        descriptor["total_chars"],
        INLINE_PREREQUISITE_RESULT_MAX_CHARS + 1
    );
    assert_eq!(descriptor["sha256"].as_str().unwrap().len(), 64);
}

fn command(id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.to_string(),
        request_fingerprint: id.to_string(),
        persist_receipt: true,
    }
}

fn graph() -> CreateTaskGraphCommand {
    let task = |task_id: &str| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "task": task_id,
            "objective": format!("Complete {task_id}"),
            "prompt": format!("Task Objective:\nComplete {task_id}")
        }),
        max_iterations: 4,
    };
    CreateTaskGraphCommand {
        graph_id: "graph-lifecycle".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![
            task("task-a"),
            task("task-b"),
            task("task-c"),
            task("task-d"),
        ],
        dependencies: vec![
            LocalTaskDependency {
                task_id: "task-b".to_string(),
                prerequisite_task_id: "task-a".to_string(),
            },
            LocalTaskDependency {
                task_id: "task-c".to_string(),
                prerequisite_task_id: "task-b".to_string(),
            },
            LocalTaskDependency {
                task_id: "task-d".to_string(),
                prerequisite_task_id: "task-c".to_string(),
            },
        ],
    }
}

fn writer_graph(
    graph_id: &str,
    source_turn_id: &str,
    task_id: &str,
    owned_paths: &[&str],
) -> CreateTaskGraphCommand {
    CreateTaskGraphCommand {
        graph_id: graph_id.to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation_turn".to_string(),
        source_entity_id: source_turn_id.to_string(),
        tasks: vec![LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({
                "objective": format!("Complete {task_id}"),
                "prompt": format!("Task Objective:\nComplete {task_id}"),
                "owned_paths": owned_paths,
                "tool_options": {
                    "enabled_builtin_kinds": ["CodeMaintainerWrite"]
                }
            }),
            max_iterations: 4,
        }],
        dependencies: Vec::new(),
    }
}

fn builtin_graph(
    graph_id: &str,
    source_turn_id: &str,
    task_id: &str,
    builtin_kind: &str,
) -> CreateTaskGraphCommand {
    CreateTaskGraphCommand {
        graph_id: graph_id.to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation_turn".to_string(),
        source_entity_id: source_turn_id.to_string(),
        tasks: vec![LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({
                "objective": format!("Complete {task_id}"),
                "prompt": format!("Task Objective:\nComplete {task_id}"),
                "tool_options": {
                    "enabled_builtin_kinds": [builtin_kind]
                }
            }),
            max_iterations: 4,
        }],
        dependencies: Vec::new(),
    }
}

#[tokio::test]
async fn task_run_prompt_discloses_frozen_owned_paths_before_any_write() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create_source_turn(
        &storage,
        "conversation-write-boundary",
        "turn-write-boundary",
        None,
        100,
    )
    .await;
    storage
        .create_task_graph(
            &command("create-write-boundary"),
            &writer_graph(
                "graph-write-boundary",
                "turn-write-boundary",
                "writer",
                &["apps/web/src/features/identity", "apps/web/tests/identity"],
            ),
            1_000,
        )
        .await
        .expect("create writer");

    let run = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-write-boundary",
            "run-write-boundary",
            "event-write-boundary",
            2_000,
        )
        .await
        .expect("start writer")
        .expect("writer run");
    let prompt = run.input["prompt"].as_str().expect("Task prompt");

    assert!(prompt.contains("[Task Write Boundary]"));
    assert!(prompt.contains("`apps/web/src/features/identity`"));
    assert!(prompt.contains("`apps/web/tests/identity`"));
    assert!(prompt.contains("do not guess sibling directories"));
}

fn dynamic_platform_graph(
    graph_id: &str,
    source_turn_id: &str,
    task_id: &str,
    plugin_keys: &[&str],
    external_mcp_ids: &[&str],
) -> CreateTaskGraphCommand {
    CreateTaskGraphCommand {
        graph_id: graph_id.to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation_turn".to_string(),
        source_entity_id: source_turn_id.to_string(),
        tasks: vec![LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({
                "objective": format!("Complete {task_id}"),
                "prompt": format!("Task Objective:\nComplete {task_id}"),
                "tool_options": {
                    "enabled_builtin_kinds": [],
                    "plugin_hints": plugin_keys.iter().map(|key| {
                        json!({"plugin_key": key, "component_key": format!("{key}-mcp")})
                    }).collect::<Vec<_>>(),
                    "external_mcp_config_ids": external_mcp_ids
                }
            }),
            max_iterations: 4,
        }],
        dependencies: Vec::new(),
    }
}

async fn create_source_turn(
    storage: &SqliteClientStorage,
    conversation_id: &str,
    turn_id: &str,
    resource: Option<LocalConversationResourceBinding>,
    now: i64,
) {
    storage
        .create_conversation(
            &command(&format!("create-{conversation_id}")),
            &CreateConversationCommand {
                conversation_id: conversation_id.to_string(),
                owner_user_id: "user-1".to_string(),
                title: conversation_id.to_string(),
                resource,
            },
            now,
        )
        .await
        .expect("create source conversation");
    let run_id = format!("main-run-{turn_id}");
    let turn = StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: conversation_id.to_string(),
        expected_conversation_version: 1,
        turn_id: turn_id.to_string(),
        message_id: format!("message-{turn_id}"),
        run_id: run_id.clone(),
        message: "Start work".to_string(),
        message_metadata: json!({}),
        attachments: Vec::new(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 4,
    };
    storage
        .start_conversation_turn(
            &command(&format!("start-{turn_id}")),
            &turn,
            &LocalAgentRunRecord {
                run_id,
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation_turn".to_string(),
                owner_entity_id: turn_id.to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "Start work"}),
                status: LocalAgentRunStatus::Queued,
                iteration: 0,
                model_attempt: 1,
                max_iterations: 4,
                version: 1,
                claim_token: None,
                claim_until_unix_ms: None,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                checkpoint: Value::Null,
                continuation_input: None,
                terminal_outcome: None,
                created_at_unix_ms: now + 1,
                updated_at_unix_ms: now + 1,
            },
            &format!("event-start-{turn_id}"),
            now + 1,
        )
        .await
        .expect("create source turn");
    storage
        .cancel_run(
            &command(&format!("cancel-main-{turn_id}")),
            &format!("main-run-{turn_id}"),
            Some(1),
            "fixture only needs the durable conversation Turn mapping",
            &format!("event-cancel-main-{turn_id}"),
            now + 2,
        )
        .await
        .expect("finish source turn fixture");
}

async fn finish_next(
    storage: &SqliteClientStorage,
    run_id: &str,
    status: LocalAgentRunStatus,
    now: i64,
) {
    finish_next_with_outcome(storage, run_id, status, json!({"status": status}), now).await;
}

async fn finish_next_with_outcome(
    storage: &SqliteClientStorage,
    run_id: &str,
    status: LocalAgentRunStatus,
    terminal_outcome: Value,
    now: i64,
) {
    storage
        .start_next_task_run("user-1", run_id, &format!("event-start-{run_id}"), now)
        .await
        .expect("start task")
        .expect("ready task");
    let claim = storage
        .claim_next_run(
            &command(&format!("claim-{run_id}")),
            "user-1",
            "worker-1",
            &format!("token-{run_id}"),
            now + 1,
            now + 10_000,
            &format!("event-claim-{run_id}"),
        )
        .await
        .expect("claim run")
        .expect("run claim");
    storage
        .apply_transition(
            &command(&format!("finish-{run_id}")),
            &RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: status,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(terminal_outcome),
                event_id: format!("event-finish-{run_id}"),
                event_type: format!("run_{}", status.as_str()),
                event_payload: json!({"status": status}),
                occurred_at_unix_ms: now + 2,
            },
        )
        .await
        .expect("finish run");
}

#[tokio::test]
async fn cross_graph_write_locks_skip_conflicts_and_release_after_cancellation() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create_source_turn(&storage, "conversation-locks", "turn-locks", None, 100).await;
    storage
        .create_task_graph(
            &command("create-writer-a"),
            &writer_graph(
                "graph-writer-a",
                "turn-locks",
                "writer-a",
                &["clients/macos"],
            ),
            1_000,
        )
        .await
        .expect("create first writer");
    storage
        .create_task_graph(
            &command("create-writer-b"),
            &writer_graph(
                "graph-writer-b",
                "turn-locks",
                "writer-b",
                &["clients/macos/Sources"],
            ),
            1_100,
        )
        .await
        .expect("create overlapping writer");
    storage
        .create_task_graph(
            &command("create-writer-c"),
            &writer_graph(
                "graph-writer-c",
                "turn-locks",
                "writer-c",
                &["clients/windows-react"],
            ),
            1_200,
        )
        .await
        .expect("create disjoint writer");

    let first = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-writer-a",
            "run-writer-a",
            "event-writer-a",
            2_000,
        )
        .await
        .expect("start first writer")
        .expect("first writer run");
    assert_eq!(first.owner_entity_id, "writer-a");
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-writer-b",
            "run-writer-b-blocked",
            "event-writer-b-blocked",
            2_100,
        )
        .await
        .expect("check overlapping writer")
        .is_none());

    let disjoint = storage
        .start_next_task_run("user-1", "run-writer-c", "event-writer-c", 2_200)
        .await
        .expect("start later disjoint writer")
        .expect("disjoint writer must bypass the blocked queue head");
    assert_eq!(disjoint.owner_entity_id, "writer-c");

    storage
        .cancel_run(
            &command("cancel-writer-a"),
            &first.run_id,
            Some(first.version),
            "release write ownership",
            "event-cancel-writer-a",
            2_300,
        )
        .await
        .expect("cancel first writer");
    let unblocked = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-writer-b",
            "run-writer-b",
            "event-writer-b",
            2_400,
        )
        .await
        .expect("start writer after lock release")
        .expect("overlapping writer should start after terminal reconciliation");
    assert_eq!(unblocked.owner_entity_id, "writer-b");
}

#[tokio::test]
async fn terminal_runtime_lock_blocks_writers_but_not_readers_and_releases() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create_source_turn(
        &storage,
        "conversation-terminal-locks",
        "turn-terminal-locks",
        None,
        100,
    )
    .await;
    storage
        .create_task_graph(
            &command("create-terminal"),
            &builtin_graph(
                "graph-terminal",
                "turn-terminal-locks",
                "terminal",
                "TerminalController",
            ),
            1_000,
        )
        .await
        .expect("create terminal Task");
    storage
        .create_task_graph(
            &command("create-writer-after-terminal"),
            &writer_graph(
                "graph-writer-after-terminal",
                "turn-terminal-locks",
                "writer-after-terminal",
                &["clients/windows-react"],
            ),
            1_100,
        )
        .await
        .expect("create writer Task");
    storage
        .create_task_graph(
            &command("create-reader-after-terminal"),
            &builtin_graph(
                "graph-reader-after-terminal",
                "turn-terminal-locks",
                "reader-after-terminal",
                "CodeMaintainerRead",
            ),
            1_200,
        )
        .await
        .expect("create reader Task");

    let terminal = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-terminal",
            "run-terminal",
            "event-terminal",
            2_000,
        )
        .await
        .expect("start terminal Task")
        .expect("terminal Run");
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-writer-after-terminal",
            "run-writer-blocked",
            "event-writer-blocked",
            2_100,
        )
        .await
        .expect("check writer while terminal is active")
        .is_none());

    let reader = storage
        .start_next_task_run(
            "user-1",
            "run-reader-after-terminal",
            "event-reader-after-terminal",
            2_200,
        )
        .await
        .expect("start reader behind blocked writer")
        .expect("reader should bypass blocked writer");
    assert_eq!(reader.owner_entity_id, "reader-after-terminal");

    storage
        .cancel_run(
            &command("cancel-terminal"),
            &terminal.run_id,
            Some(terminal.version),
            "release conservative terminal lock",
            "event-cancel-terminal",
            2_300,
        )
        .await
        .expect("cancel terminal Run");
    let writer = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-writer-after-terminal",
            "run-writer-after-terminal",
            "event-writer-after-terminal",
            2_400,
        )
        .await
        .expect("start writer after terminal lock release")
        .expect("writer should be unblocked");
    assert_eq!(writer.owner_entity_id, "writer-after-terminal");
}

#[tokio::test]
async fn plugin_and_external_mcp_runtime_locks_serialize_native_writers() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create_source_turn(
        &storage,
        "conversation-dynamic-locks",
        "turn-dynamic-locks",
        None,
        100,
    )
    .await;
    storage
        .create_task_graph(
            &command("create-plugin"),
            &dynamic_platform_graph(
                "graph-plugin",
                "turn-dynamic-locks",
                "plugin-task",
                &["document"],
                &[],
            ),
            1_000,
        )
        .await
        .expect("create Plugin Task");
    storage
        .create_task_graph(
            &command("create-external"),
            &dynamic_platform_graph(
                "graph-external",
                "turn-dynamic-locks",
                "external-task",
                &[],
                &["external-1"],
            ),
            1_100,
        )
        .await
        .expect("create External MCP Task");
    storage
        .create_task_graph(
            &command("create-reader-after-plugin"),
            &builtin_graph(
                "graph-reader-after-plugin",
                "turn-dynamic-locks",
                "reader-after-plugin",
                "CodeMaintainerRead",
            ),
            1_200,
        )
        .await
        .expect("create reader Task");

    let plugin = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-plugin",
            "run-plugin",
            "event-plugin",
            2_000,
        )
        .await
        .expect("start Plugin Task")
        .expect("Plugin Run");
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-external",
            "run-external-blocked",
            "event-external-blocked",
            2_100,
        )
        .await
        .expect("check External MCP Task while Plugin is active")
        .is_none());

    let reader = storage
        .start_next_task_run(
            "user-1",
            "run-reader-after-plugin",
            "event-reader-after-plugin",
            2_200,
        )
        .await
        .expect("start reader behind blocked native writer")
        .expect("reader should bypass blocked native writer");
    assert_eq!(reader.owner_entity_id, "reader-after-plugin");

    storage
        .cancel_run(
            &command("cancel-plugin"),
            &plugin.run_id,
            Some(plugin.version),
            "release conservative Plugin lock",
            "event-cancel-plugin",
            2_300,
        )
        .await
        .expect("cancel Plugin Run");
    let external = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-external",
            "run-external",
            "event-external",
            2_400,
        )
        .await
        .expect("start External MCP Task after lock release")
        .expect("External MCP Task should be unblocked");
    assert_eq!(external.owner_entity_id, "external-task");
}

#[tokio::test]
async fn different_plugin_runtime_locks_run_concurrently_while_same_plugin_waits() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create_source_turn(
        &storage,
        "conversation-plugin-concurrency",
        "turn-plugin-concurrency",
        None,
        100,
    )
    .await;
    for (graph_id, task_id, plugin_key, created_at) in [
        ("graph-document-a", "document-a", "document", 1_000),
        ("graph-browser", "browser", "browser", 1_100),
        ("graph-document-b", "document-b", "document", 1_200),
    ] {
        storage
            .create_task_graph(
                &command(&format!("create-{task_id}")),
                &dynamic_platform_graph(
                    graph_id,
                    "turn-plugin-concurrency",
                    task_id,
                    &[plugin_key],
                    &[],
                ),
                created_at,
            )
            .await
            .expect("create Plugin Task");
    }

    let document = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-document-a",
            "run-document-a",
            "event-document-a",
            2_000,
        )
        .await
        .expect("start first Document Task")
        .expect("first Document Run");
    let browser = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-browser",
            "run-browser",
            "event-browser",
            2_100,
        )
        .await
        .expect("start Browser Task")
        .expect("Browser should not share the Document runtime lock");
    assert_eq!(browser.owner_entity_id, "browser");

    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-document-b",
            "run-document-b-blocked",
            "event-document-b-blocked",
            2_200,
        )
        .await
        .expect("check second Document Task")
        .is_none());

    storage
        .cancel_run(
            &command("cancel-document-a"),
            &document.run_id,
            Some(document.version),
            "release Document runtime lock",
            "event-cancel-document-a",
            2_300,
        )
        .await
        .expect("cancel first Document Run");
    let second_document = storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-document-b",
            "run-document-b",
            "event-document-b",
            2_400,
        )
        .await
        .expect("start second Document Task")
        .expect("second Document Task should start after the matching lock is released");
    assert_eq!(second_document.owner_entity_id, "document-b");
}

#[tokio::test]
async fn write_locks_are_isolated_by_conversation_and_use_project_scope_keys() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create_source_turn(&storage, "conversation-a", "turn-a", None, 100).await;
    create_source_turn(&storage, "conversation-b", "turn-b", None, 200).await;
    for (graph_id, turn_id, task_id, created_at) in [
        (
            "graph-conversation-a",
            "turn-a",
            "writer-conversation-a",
            1_000,
        ),
        (
            "graph-conversation-b",
            "turn-b",
            "writer-conversation-b",
            1_100,
        ),
    ] {
        storage
            .create_task_graph(
                &command(&format!("create-{graph_id}")),
                &writer_graph(graph_id, turn_id, task_id, &["shared/path"]),
                created_at,
            )
            .await
            .expect("create conversation writer");
    }
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-conversation-a",
            "run-conversation-a",
            "event-conversation-a",
            2_000,
        )
        .await
        .expect("start first conversation writer")
        .is_some());
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-conversation-b",
            "run-conversation-b",
            "event-conversation-b",
            2_100,
        )
        .await
        .expect("start second conversation writer")
        .is_some());

    let project = |resource_id: &str| LocalConversationResourceBinding {
        kind: LocalConversationResourceKind::Project,
        resource_id: resource_id.to_string(),
    };
    create_source_turn(
        &storage,
        "project-conversation-a",
        "project-turn-a",
        Some(project("project-1")),
        300,
    )
    .await;
    let mut project_conversation_graph = writer_graph(
        "graph-project-a",
        "project-turn-a",
        "writer-project-a",
        &["shared/path"],
    );
    project_conversation_graph.source_entity_type = "conversation".to_string();
    project_conversation_graph.source_entity_id = "project-conversation-a".to_string();
    storage
        .create_task_graph(
            &command("create-project-writer-a"),
            &project_conversation_graph,
            1_200,
        )
        .await
        .expect("create first project writer");
    storage
        .create_task_graph(
            &command("create-project-writer-b"),
            &writer_graph(
                "graph-project-b",
                "project-turn-a",
                "writer-project-b",
                &["shared/path/child"],
            ),
            1_300,
        )
        .await
        .expect("create second project writer");
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-project-a",
            "run-project-a",
            "event-project-a",
            2_200,
        )
        .await
        .expect("start first project writer")
        .is_some());
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-project-b",
            "run-project-b-blocked",
            "event-project-b-blocked",
            2_300,
        )
        .await
        .expect("check second project writer")
        .is_none());
}

#[tokio::test]
async fn expired_writer_model_claim_requeues_and_keeps_its_cross_graph_lock() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create_source_turn(
        &storage,
        "conversation-recovery",
        "turn-recovery",
        None,
        100,
    )
    .await;
    for (graph_id, task_id, created_at) in [
        ("graph-expiring-writer", "expiring-writer", 1_000),
        ("graph-waiting-writer", "waiting-writer", 1_100),
    ] {
        storage
            .create_task_graph(
                &command(&format!("create-{graph_id}")),
                &writer_graph(graph_id, "turn-recovery", task_id, &["clients/macos"]),
                created_at,
            )
            .await
            .expect("create recovery writer");
    }
    storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-expiring-writer",
            "run-expiring-writer",
            "event-expiring-writer",
            2_000,
        )
        .await
        .expect("start expiring writer")
        .expect("expiring writer run");
    let claim = storage
        .claim_next_run(
            &command("claim-expiring-writer"),
            "user-1",
            "worker-1",
            "token-expiring-writer",
            2_001,
            3_000,
            "event-claim-expiring-writer",
        )
        .await
        .expect("claim expiring writer")
        .expect("writer claim");
    assert_eq!(claim.run.run_id, "run-expiring-writer");
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-waiting-writer",
            "run-waiting-blocked",
            "event-waiting-blocked",
            2_500,
        )
        .await
        .expect("check waiting writer")
        .is_none());

    assert_eq!(
        storage
            .recover_expired_claims("user-1", 3_001)
            .await
            .expect("recover expired writer"),
        1
    );
    assert!(storage
        .start_next_task_run_for_graph(
            "user-1",
            "graph-waiting-writer",
            "run-waiting-writer",
            "event-waiting-writer",
            3_100,
        )
        .await
        .expect("check waiting writer after recovery")
        .is_none());
    let recovered = storage
        .get_run("run-expiring-writer")
        .await
        .expect("get recovered writer")
        .expect("recovered writer");
    assert_eq!(recovered.status, LocalAgentRunStatus::ModelReady);
}

#[tokio::test]
async fn terminal_runs_unlock_or_block_the_remaining_dag() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-graph"), &graph(), 1_000)
        .await
        .expect("create graph");

    finish_next(&storage, "run-a", LocalAgentRunStatus::Succeeded, 2_000).await;
    let after_success = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(after_success.status, LocalTaskGraphStatus::Running);
    assert_eq!(after_success.tasks[0].status, LocalTaskStatus::Succeeded);
    assert_eq!(after_success.tasks[1].status, LocalTaskStatus::Ready);
    assert_eq!(after_success.tasks[2].status, LocalTaskStatus::Pending);

    finish_next(&storage, "run-b", LocalAgentRunStatus::Failed, 3_000).await;
    let after_failure = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(after_failure.status, LocalTaskGraphStatus::Failed);
    assert_eq!(after_failure.tasks[1].status, LocalTaskStatus::Failed);
    assert_eq!(after_failure.tasks[2].status, LocalTaskStatus::Blocked);
    assert_eq!(after_failure.tasks[3].status, LocalTaskStatus::Blocked);
    assert!(storage
        .start_next_task_run("user-1", "run-none", "event-none", 4_000)
        .await
        .expect("start next")
        .is_none());
}

#[tokio::test]
async fn generic_run_cancellation_reconciles_its_task_graph() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-cancel-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    let run = storage
        .start_next_task_run("user-1", "run-cancel", "event-start-cancel", 2_000)
        .await
        .expect("start task")
        .expect("ready task");
    storage
        .cancel_run(
            &command("cancel-run"),
            &run.run_id,
            Some(run.version),
            "user cancelled Run",
            "event-cancel-run",
            3_000,
        )
        .await
        .expect("cancel Run");

    let graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(graph.status, LocalTaskGraphStatus::Cancelled);
    assert_eq!(graph.tasks[0].status, LocalTaskStatus::Cancelled);
    assert!(graph.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Blocked));
}

#[tokio::test]
async fn dependent_task_receives_only_user_visible_prerequisite_results() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let mut context_graph = graph();
    context_graph.tasks[0].input["acceptance_report_version"] = json!(1);
    context_graph.tasks[0].input["acceptance_criteria"] = json!(["Model defined"]);
    context_graph.tasks[0].input["final_response_max_chars"] = json!(100);
    context_graph.tasks[1].input["owned_paths"] =
        json!(["apps/web/src/features/identity", "apps/web/tests/identity"]);
    storage
        .create_task_graph(&command("create-context-graph"), &context_graph, 1_000)
        .await
        .expect("create graph");
    finish_next_with_outcome(
        &storage,
        "run-prerequisite",
        LocalAgentRunStatus::Succeeded,
        json!({
            "content": "Implemented the data model.",
            "report": {"content": "Tests: 12 passed"},
            "task_acceptance":{"status":"succeeded","final_response":"Implemented the data model.","final_response_chars":999,"acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Checked result"}]},
            "reasoning": "hidden chain of thought must never be forwarded"
        }),
        2_000,
    )
    .await;

    let dependent = storage
        .start_next_task_run("user-1", "run-dependent", "event-start-dependent", 3_000)
        .await
        .expect("start dependent")
        .expect("dependent run");
    let prompt = dependent.input["prompt"].as_str().expect("Task prompt");
    assert!(prompt.contains("Task Acceptance Facts"));
    assert_eq!(
        dependent.input["resolved_prerequisites"][0]["acceptance_summary"]["final_response_chars"],
        27
    );
    assert_eq!(
        dependent.input["resolved_prerequisites"][0]["acceptance_summary"]
            ["final_response_max_chars"],
        100
    );

    assert!(prompt.contains("[Prerequisite Task Results]"));
    assert!(prompt.contains("Implemented the data model."));
    assert!(prompt.contains("Tests: 12 passed"));
    assert!(prompt.contains("[Current Task]"));
    assert!(prompt.contains("[Task Write Boundary]"));
    assert!(prompt.contains("`apps/web/src/features/identity`"));
    assert!(prompt.contains("`apps/web/tests/identity`"));
    assert!(!prompt.contains("hidden chain of thought"));
    assert_eq!(
        dependent.input["resolved_prerequisites"][0]["run_id"],
        "run-prerequisite"
    );
}

#[tokio::test]
async fn existing_task_ids_preserve_cross_graph_dependency_execution_and_context() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let task = |task_id: &str, prerequisites: Value| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "objective": format!("Complete {task_id}"),
            "prompt": format!("Task Objective:\nComplete {task_id}"),
            "prerequisite_task_ids": prerequisites,
        }),
        max_iterations: 4,
    };
    storage
        .create_task_graph(
            &command("create-existing-prerequisite"),
            &CreateTaskGraphCommand {
                graph_id: "graph-existing-prerequisite".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation".to_string(),
                source_entity_id: "conversation-1".to_string(),
                tasks: vec![task("existing-prerequisite", json!([]))],
                dependencies: Vec::new(),
            },
            1_000,
        )
        .await
        .expect("create prerequisite graph");
    let dependent = storage
        .create_task_graph(
            &command("create-cross-graph-dependent"),
            &CreateTaskGraphCommand {
                graph_id: "graph-cross-dependent".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation".to_string(),
                source_entity_id: "conversation-2".to_string(),
                tasks: vec![task("cross-dependent", json!(["existing-prerequisite"]))],
                dependencies: Vec::new(),
            },
            1_100,
        )
        .await
        .expect("create dependent graph");
    assert_eq!(dependent.tasks[0].status, LocalTaskStatus::Pending);

    finish_next_with_outcome(
        &storage,
        "run-existing-prerequisite",
        LocalAgentRunStatus::Succeeded,
        json!({"content": "Existing prerequisite result"}),
        2_000,
    )
    .await;

    let dependent = storage
        .start_next_task_run(
            "user-1",
            "run-cross-dependent",
            "event-start-cross-dependent",
            3_000,
        )
        .await
        .expect("start cross-graph dependent")
        .expect("dependent is ready");
    assert_eq!(dependent.owner_entity_id, "cross-dependent");
    assert!(dependent.input["prompt"]
        .as_str()
        .is_some_and(|prompt| prompt.contains("Existing prerequisite result")));
    assert_eq!(
        dependent.input["resolved_prerequisites"][0]["task_id"],
        "existing-prerequisite"
    );
}

#[tokio::test]
async fn root_task_input_is_not_rewritten_without_prerequisites() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-root-graph"), &graph(), 1_000)
        .await
        .expect("create graph");

    let root = storage
        .start_next_task_run("user-1", "run-root", "event-start-root", 2_000)
        .await
        .expect("start root")
        .expect("root run");

    assert_eq!(
        root.input,
        json!({
            "task": "task-a",
            "objective": "Complete task-a",
            "prompt": "Task Objective:\nComplete task-a"
        })
    );
}

#[tokio::test]
async fn completed_external_dependencies_are_ready_on_creation_and_old_pending_is_owner_scoped() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    for owner in ["user-1", "user-2"] {
        let mut source = graph();
        source.graph_id = format!("source-{owner}");
        source.owner_user_id = owner.into();
        source.tasks.truncate(1);
        source.tasks[0].task_id = format!("source-task-{owner}");
        source.dependencies.clear();
        storage
            .create_task_graph(&command(&format!("create-source-{owner}")), &source, 1000)
            .await
            .unwrap();
        // Isolated fixture terminal state; never used on the real database.
        sqlx::query("UPDATE local_tasks SET status='succeeded' WHERE task_id=?")
            .bind(&source.tasks[0].task_id)
            .execute(&storage.pool)
            .await
            .unwrap();
        let mut dependent = source.clone();
        dependent.graph_id = format!("dependent-{owner}");
        dependent.tasks[0].task_id = format!("dependent-task-{owner}");
        dependent.tasks[0].input["prerequisite_task_ids"] = json!([source.tasks[0].task_id]);
        let created = storage
            .create_task_graph(
                &command(&format!("create-dependent-{owner}")),
                &dependent,
                2000,
            )
            .await
            .unwrap();
        assert_eq!(created.tasks[0].status, LocalTaskStatus::Ready);
        sqlx::query("UPDATE local_tasks SET status='pending' WHERE task_id=?")
            .bind(&dependent.tasks[0].task_id)
            .execute(&storage.pool)
            .await
            .unwrap();
    }
    let started = storage
        .start_next_task_run("user-1", "recovered-run", "recovered-event", 3000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(started.owner_entity_id, "dependent-task-user-1");
    let other = storage
        .get_task_graph("user-2", "dependent-user-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.tasks[0].status, LocalTaskStatus::Pending);
}

#[test]
fn prerequisite_acceptance_facts_recount_delivery_not_worker_estimates() {
    let input = json!({"acceptance_report_version":1,"acceptance_criteria":["Review"],"final_response_max_chars":4});
    let outcome = json!({"content":"中文\n!","task_acceptance":{"status":"succeeded","final_response":"中文\n!","final_response_chars":999,"acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Estimated 999 characters"}]}});
    let summary = prerequisite_acceptance_summary(&input, &outcome).unwrap();
    assert_eq!(summary["final_response_chars"], 4);
    assert_eq!(summary["explicit_length_limit_passed"], true);
    assert_eq!(summary["result_summary_is_complete_final_response"], true);
    assert_eq!(
        summary["content_verification"],
        "worker_reported_not_independently_certified"
    );
    assert!(prerequisite_acceptance_summary(&json!({}), &outcome).is_none());
    let mut rewritten = outcome.clone();
    rewritten["content"] = json!("Different text");
    assert!(prerequisite_acceptance_summary(&input, &rewritten).is_none());
}

#[tokio::test]
async fn future_contact_async_task_waits_until_its_run_at_deadline() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let mut scheduled = graph();
    scheduled.graph_id = "graph-scheduled".to_string();
    scheduled.tasks.truncate(1);
    scheduled.dependencies.clear();
    scheduled.tasks[0].input["schedule"] = json!({
        "mode": "contact_async",
        "run_at": "1970-01-01T00:00:05Z",
        "run_at_unix_ms": 5_000,
    });
    storage
        .create_task_graph(&command("create-scheduled-graph"), &scheduled, 1_000)
        .await
        .expect("create scheduled graph");

    assert!(storage
        .start_next_task_run("user-1", "run-too-early", "event-too-early", 4_999)
        .await
        .expect("check early schedule")
        .is_none());
    let run = storage
        .start_next_task_run("user-1", "run-on-time", "event-on-time", 5_000)
        .await
        .expect("start due schedule")
        .expect("scheduled Task is due");
    assert_eq!(run.owner_entity_id, "task-a");
}

#[tokio::test]
async fn ai_reported_blocked_outcome_overrides_a_successful_model_run() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-reported-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    storage
        .start_next_task_run(
            "user-1",
            "run-reported-blocked",
            "event-start-reported",
            2_000,
        )
        .await
        .expect("start task")
        .expect("ready task");
    let claim = storage
        .claim_next_run(
            &command("claim-reported"),
            "user-1",
            "worker-1",
            "token-reported",
            2_001,
            12_000,
            "event-claim-reported",
        )
        .await
        .expect("claim")
        .expect("run claim");
    sqlx::query(
        "INSERT INTO local_agent_tool_invocations(\
             invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
             side_effecting, requires_approval, approval_status, status, result_json, error_text, \
             version, created_at_unix_ms, updated_at_unix_ms) \
             VALUES('outcome-1', ?, 'batch-1', 'call-1', ?, ?, 0, 0, 'not_required', \
             'succeeded', '{}', NULL, 2, 2002, 2002)",
    )
    .bind(&claim.run.run_id)
    .bind(TASK_OUTCOME_REPORT_TOOL)
    .bind(r#"{"status":"blocked","reason":"Waiting for a required credential."}"#)
    .execute(&storage.pool)
    .await
    .expect("store reported outcome");
    storage
        .apply_transition(
            &command("finish-reported"),
            &RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::Succeeded,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({"content": "Model response"})),
                event_id: "event-finish-reported".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({"status": "succeeded"}),
                occurred_at_unix_ms: 2_003,
            },
        )
        .await
        .expect("finish run");

    let graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(graph.tasks[0].status, LocalTaskStatus::Blocked);
    assert!(graph.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Blocked));

    let retried = storage
        .retry_task(
            &command("retry-reported-block"),
            "user-1",
            &graph.tasks[0].task_id,
            graph.tasks[0].version,
            Some("Use the corrected date range."),
            3_000,
        )
        .await
        .expect("retry explicitly blocked task");
    assert_eq!(retried.tasks[0].status, LocalTaskStatus::Ready);
    assert!(retried.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Pending));
}

#[tokio::test]
async fn expired_model_claim_requeues_without_blocking_its_task() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-expired-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    let run = storage
        .start_next_task_run(
            "user-1",
            "run-expired-task",
            "event-start-expired-task",
            2_000,
        )
        .await
        .expect("start task")
        .expect("ready task");
    storage
        .claim_next_run(
            &command("claim-expired-task"),
            "user-1",
            "worker-1",
            "claim-expired-task",
            2_001,
            3_000,
            "event-claim-expired-task",
        )
        .await
        .expect("claim task run")
        .expect("claimed task run");

    assert_eq!(
        storage
            .recover_expired_claims("user-1", 3_001)
            .await
            .expect("recover expired task run"),
        1
    );

    let recovered_run = storage
        .get_run(&run.run_id)
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(recovered_run.status, LocalAgentRunStatus::ModelReady);
    assert_eq!(recovered_run.iteration, 0);
    assert!(recovered_run.terminal_outcome.is_none());
    let recovered_graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(recovered_graph.tasks[0].status, LocalTaskStatus::Running);
    assert_eq!(
        recovered_graph.tasks[0].active_run_id.as_deref(),
        Some(run.run_id.as_str())
    );
    assert!(recovered_graph.tasks[1..]
        .iter()
        .all(|task| task.status == LocalTaskStatus::Pending));
}

#[tokio::test]
async fn startup_repairs_a_legacy_needs_review_run_with_a_running_task() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_task_graph(&command("create-stranded-graph"), &graph(), 1_000)
        .await
        .expect("create graph");
    let run = storage
        .start_next_task_run(
            "user-1",
            "run-stranded-task",
            "event-start-stranded-task",
            2_000,
        )
        .await
        .expect("start task")
        .expect("ready task");
    storage
        .claim_next_run(
            &command("claim-stranded-task"),
            "user-1",
            "worker-1",
            "claim-stranded-task",
            2_001,
            3_000,
            "event-claim-stranded-task",
        )
        .await
        .expect("claim task run")
        .expect("claimed task run");
    sqlx::query(
        "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
         claim_token = NULL, claim_until_unix_ms = NULL WHERE run_id = ?",
    )
    .bind(&run.run_id)
    .execute(&storage.pool)
    .await
    .expect("simulate legacy recovery");

    assert_eq!(
        storage
            .recover_expired_claims("user-1", 3_001)
            .await
            .expect("repair stranded task run"),
        1
    );

    let recovered_graph = storage
        .get_task_graph("user-1", "graph-lifecycle")
        .await
        .expect("get graph")
        .expect("graph");
    assert_eq!(recovered_graph.tasks[0].status, LocalTaskStatus::Blocked);
    assert!(recovered_graph.tasks[0].active_run_id.is_none());
}

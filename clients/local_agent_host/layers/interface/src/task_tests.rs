use super::*;
use crate::task_write_scope::{
    local_task_runtime_write_scope, local_task_write_scope, local_task_write_scope_overlap,
};
use serde_json::json;

fn record(status: LocalTaskStatus) -> LocalTaskRecord {
    LocalTaskRecord {
        graph_id: "graph-1".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        task_id: format!("task-{}", status.as_str()),
        title: "Task".to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: Value::Null,
        max_iterations: 4,
        status,
        active_run_id: None,
        result_summary: None,
        version: 1,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    }
}

fn task(task_id: &str) -> LocalTaskSpec {
    LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({"prompt": task_id}),
        max_iterations: 8,
    }
}

fn command(dependencies: Vec<LocalTaskDependency>) -> CreateTaskGraphCommand {
    CreateTaskGraphCommand {
        graph_id: "graph-1".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![task("task-1"), task("task-2")],
        dependencies,
    }
}

fn write_task(task_id: &str, owned_paths: &[&str]) -> LocalTaskSpec {
    let mut value = task(task_id);
    value.input = json!({
        "prompt": task_id,
        "owned_paths": owned_paths,
        "tool_options": {
            "enabled_builtin_kinds": ["CodeMaintainerWrite"]
        }
    });
    value
}

fn terminal_task(task_id: &str) -> LocalTaskSpec {
    let mut value = task(task_id);
    value.input = json!({
        "prompt": task_id,
        "tool_options": {
            "enabled_builtin_kinds": ["TerminalController"]
        }
    });
    value
}

#[test]
fn validates_acyclic_graph_and_rejects_cycle() {
    assert!(command(vec![LocalTaskDependency {
        task_id: "task-2".to_string(),
        prerequisite_task_id: "task-1".to_string(),
    }])
    .validate()
    .is_ok());
    assert!(command(vec![
        LocalTaskDependency {
            task_id: "task-2".to_string(),
            prerequisite_task_id: "task-1".to_string(),
        },
        LocalTaskDependency {
            task_id: "task-1".to_string(),
            prerequisite_task_id: "task-2".to_string(),
        },
    ])
    .validate()
    .is_err());
}

#[test]
fn rejects_unordered_overlapping_writers_and_accepts_safe_ownership() {
    let graph = |left: &[&str], right: &[&str], ordered: bool| CreateTaskGraphCommand {
        graph_id: "graph-write".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![write_task("writer-a", left), write_task("writer-b", right)],
        dependencies: ordered
            .then(|| LocalTaskDependency {
                task_id: "writer-b".to_string(),
                prerequisite_task_id: "writer-a".to_string(),
            })
            .into_iter()
            .collect(),
    };

    assert!(graph(&["clients/macos"], &["clients/windows-react"], false)
        .validate()
        .is_ok());
    let overlap = graph(&["clients"], &["clients/macos/App.swift"], false)
        .validate()
        .expect_err("ancestor paths must conflict");
    assert!(overlap.contains("overlapping owned_paths"));
    assert!(graph(&[], &["clients/windows-react"], false)
        .validate()
        .is_err());
    assert!(graph(&["clients"], &["clients/macos"], true)
        .validate()
        .is_ok());
}

#[test]
fn terminal_is_a_runtime_only_entire_project_writer() {
    let terminal = terminal_task("terminal");
    assert_eq!(
        local_task_write_scope(&terminal.input, &terminal.task_id).expect("static scope"),
        None
    );
    assert_eq!(
        local_task_runtime_write_scope(&terminal.input, &terminal.task_id).expect("runtime scope"),
        Some(Vec::new())
    );
    let mut terminal_with_narrow_code_ownership = write_task("mixed", &["clients/macos"]);
    terminal_with_narrow_code_ownership.input["tool_options"]["enabled_builtin_kinds"] =
        json!(["CodeMaintainerWrite", "TerminalController"]);
    assert_eq!(
        local_task_runtime_write_scope(
            &terminal_with_narrow_code_ownership.input,
            &terminal_with_narrow_code_ownership.task_id,
        )
        .expect("mixed runtime scope"),
        Some(Vec::new())
    );

    let graph = CreateTaskGraphCommand {
        graph_id: "graph-terminal".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![terminal.clone(), terminal_task("terminal-2")],
        dependencies: Vec::new(),
    };
    assert!(graph.validate().is_ok());
}

#[test]
fn plugins_lock_their_own_runtime_and_external_mcp_locks_the_project() {
    let capability_task = |task_id: &str, tool_options: Value| LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "objective": format!("Complete {task_id}"),
            "tool_options": tool_options
        }),
        max_iterations: 4,
    };
    let document = capability_task(
        "document",
        json!({
            "enabled_builtin_kinds": [],
            "plugin_hints": [{"plugin_key": "document", "component_key": "document-mcp"}],
            "external_mcp_config_ids": []
        }),
    );
    let browser = capability_task(
        "browser",
        json!({
            "enabled_builtin_kinds": [],
            "plugin_hints": [{"plugin_key": "browser", "component_key": "browser-mcp"}],
            "external_mcp_config_ids": []
        }),
    );
    let document_scope = local_task_runtime_write_scope(&document.input, &document.task_id)
        .expect("document runtime scope")
        .expect("document lock");
    let browser_scope = local_task_runtime_write_scope(&browser.input, &browser.task_id)
        .expect("browser runtime scope")
        .expect("browser lock");
    assert_eq!(document_scope.len(), 1);
    assert_eq!(browser_scope.len(), 1);
    assert!(local_task_write_scope_overlap(&document_scope, &browser_scope).is_none());

    let another_document = capability_task(
        "document-2",
        json!({
            "enabled_builtin_kinds": [],
            "plugin_hints": [{"plugin_key": "document", "component_key": "document-mcp"}],
            "external_mcp_config_ids": []
        }),
    );
    let another_document_scope =
        local_task_runtime_write_scope(&another_document.input, &another_document.task_id)
            .expect("second document runtime scope")
            .expect("second document lock");
    assert!(local_task_write_scope_overlap(&document_scope, &another_document_scope).is_some());

    let external = capability_task(
        "external",
        json!({
            "enabled_builtin_kinds": [],
            "plugin_hints": [],
            "external_mcp_config_ids": ["external-1"]
        }),
    );
    for task in [&document, &browser, &external] {
        assert_eq!(
            local_task_write_scope(&task.input, &task.task_id).expect("static scope"),
            None
        );
    }
    assert_eq!(
        local_task_runtime_write_scope(&external.input, &external.task_id)
            .expect("external runtime scope"),
        Some(Vec::new())
    );

    let graph = CreateTaskGraphCommand {
        graph_id: "graph-dynamic-platform".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![document, browser, external],
        dependencies: Vec::new(),
    };
    assert!(graph.validate().is_ok());

    let malformed = capability_task("malformed", json!({"plugin_hints": "document"}));
    assert!(local_task_runtime_write_scope(&malformed.input, &malformed.task_id).is_err());
    for options in [
        json!({"plugin_hints": [{"plugin_key": "document"}]}),
        json!({"plugin_hints": [{"plugin_key": "document", "component_key": "document-mcp"}], "external_mcp_config_ids": "bad"}),
        json!({"enabled_builtin_kinds": ["TerminalController"], "plugin_hints": "bad"}),
    ] {
        let malformed = capability_task("malformed-selection", options);
        assert!(local_task_runtime_write_scope(&malformed.input, &malformed.task_id).is_err());
    }
}

#[test]
fn rejects_non_normalized_owned_paths() {
    for path in [
        "/tmp/file",
        "../secret",
        "clients//macos",
        "C:/repo/file",
        "C:repo/file",
        "~/repo/file",
        "a\\b",
    ] {
        let graph = CreateTaskGraphCommand {
            graph_id: "graph-invalid-path".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![write_task("writer", &[path])],
            dependencies: vec![],
        };
        assert!(graph.validate().is_err(), "path should be rejected: {path}");
    }
}

#[test]
fn notification_policy_defaults_by_graph_shape_and_requires_consistency() {
    assert_eq!(
        LocalTaskGraphNotificationPolicy::default(),
        LocalTaskGraphNotificationPolicy::ProgressSummary
    );
    assert_eq!(
        "progress_summary"
            .parse::<LocalTaskGraphNotificationPolicy>()
            .unwrap()
            .as_str(),
        "progress_summary"
    );
    let single = vec![task("only")];
    assert_eq!(
        resolve_local_task_graph_notification_policy(
            single.iter().map(|task| &task.input),
            single.len(),
        )
        .expect("single policy"),
        LocalTaskGraphNotificationPolicy::AllTasks
    );
    let mut multiple = vec![task("first"), task("second")];
    assert_eq!(
        resolve_local_task_graph_notification_policy(
            multiple.iter().map(|task| &task.input),
            multiple.len(),
        )
        .expect("batch policy"),
        LocalTaskGraphNotificationPolicy::GraphSummary
    );
    for task in &mut multiple {
        task.input["graph_notification_policy"] = json!("milestones");
        task.input["notification_milestone"] = json!(task.task_id == "second");
    }
    assert_eq!(
        resolve_local_task_graph_notification_policy(
            multiple.iter().map(|task| &task.input),
            multiple.len(),
        )
        .expect("explicit policy"),
        LocalTaskGraphNotificationPolicy::Milestones
    );
    multiple[1].input["graph_notification_policy"] = json!("all_tasks");
    let inconsistent = CreateTaskGraphCommand {
        graph_id: "graph-notifications".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: multiple,
        dependencies: Vec::new(),
    };
    assert!(inconsistent.validate().is_err());
}

#[test]
fn final_delivery_requires_one_boolean_sink_covering_the_graph() {
    let mut graph = CreateTaskGraphCommand {
        graph_id: "delivery-graph".to_string(),
        owner_user_id: "user-1".to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: "conversation-1".to_string(),
        tasks: vec![task("a"), task("b"), task("final")],
        dependencies: vec![
            LocalTaskDependency {
                task_id: "b".to_string(),
                prerequisite_task_id: "a".to_string(),
            },
            LocalTaskDependency {
                task_id: "final".to_string(),
                prerequisite_task_id: "b".to_string(),
            },
        ],
    };
    graph.tasks[2].input["final_delivery"] = json!(true);
    assert!(graph.validate().is_ok()); // Transitive coverage counts.
    graph.dependencies.pop();
    assert!(graph.validate().unwrap_err().contains("depend on every"));
    graph.tasks[1].input["final_delivery"] = json!(true);
    assert!(graph.validate().unwrap_err().contains("only one"));
    graph.tasks[1].input["final_delivery"] = json!("true");
    assert!(graph.validate().unwrap_err().contains("boolean"));
}

#[test]
fn derives_task_graph_lifecycle_status() {
    let derive = |statuses: &[LocalTaskStatus]| {
        LocalTaskGraphStatus::derive(&statuses.iter().copied().map(record).collect::<Vec<_>>())
    };
    assert_eq!(
        derive(&[LocalTaskStatus::Ready, LocalTaskStatus::Pending]),
        LocalTaskGraphStatus::Pending
    );
    assert_eq!(
        derive(&[LocalTaskStatus::Running, LocalTaskStatus::Pending]),
        LocalTaskGraphStatus::Running
    );
    assert_eq!(
        derive(&[LocalTaskStatus::Succeeded, LocalTaskStatus::Ready]),
        LocalTaskGraphStatus::Running
    );
    assert_eq!(
        derive(&[LocalTaskStatus::Succeeded, LocalTaskStatus::Succeeded]),
        LocalTaskGraphStatus::Succeeded
    );
    assert_eq!(
        derive(&[LocalTaskStatus::Failed, LocalTaskStatus::Blocked]),
        LocalTaskGraphStatus::Failed
    );
    assert_eq!(
        derive(&[LocalTaskStatus::Cancelled, LocalTaskStatus::Blocked]),
        LocalTaskGraphStatus::Cancelled
    );
}

#[test]
fn validates_task_run_page_limit() {
    assert!(GetTaskRunsCommand {
        owner_user_id: "user-1".to_string(),
        task_id: "task-1".to_string(),
        limit: 100,
    }
    .validate()
    .is_ok());
    assert!(GetTaskRunsCommand {
        owner_user_id: "user-1".to_string(),
        task_id: "task-1".to_string(),
        limit: 0,
    }
    .validate()
    .is_err());
}

#[test]
fn validates_task_graph_list_cursor_and_owner() {
    let valid = ListTaskGraphsCommand {
        owner_user_id: "user-1".to_string(),
        scope: LocalTaskGraphListScope::Active,
        source_entity_type: Some("conversation_turn".to_string()),
        source_entity_id: Some("turn-1".to_string()),
        before_updated_at_unix_ms: Some(1_000),
        before_graph_id: Some("graph-1".to_string()),
        limit: 25,
    };
    assert!(valid.validate().is_ok());
    assert!(ListTaskGraphsCommand {
        before_graph_id: None,
        ..valid.clone()
    }
    .validate()
    .is_err());
    assert!(ListTaskGraphsCommand {
        owner_user_id: String::new(),
        ..valid.clone()
    }
    .validate()
    .is_err());
    assert!(ListTaskGraphsCommand {
        source_entity_id: None,
        ..valid.clone()
    }
    .validate()
    .is_err());
    assert!(ListTaskGraphsCommand {
        limit: 101,
        ..valid
    }
    .validate()
    .is_err());
}

#[test]
fn validates_force_restart_reason_and_version() {
    assert!(RestartTaskCommand {
        owner_user_id: "user-1".to_string(),
        task_id: "task-1".to_string(),
        expected_version: 2,
        reason: "rerun with fresh outputs".to_string(),
    }
    .validate()
    .is_ok());
    assert!(RestartTaskCommand {
        owner_user_id: "user-1".to_string(),
        task_id: "task-1".to_string(),
        expected_version: 0,
        reason: "rerun".to_string(),
    }
    .validate()
    .is_err());
    assert!(RestartTaskCommand {
        owner_user_id: "user-1".to_string(),
        task_id: "task-1".to_string(),
        expected_version: 2,
        reason: " ".to_string(),
    }
    .validate()
    .is_err());
}

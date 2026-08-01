// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::*;

use super::*;
use crate::catalog::TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY;

#[test]
fn legacy_agent_iteration_values_collapse_to_one_key() {
    let mut values = BTreeMap::from([
        ("chatos.ai.max_iterations".to_string(), json!(700)),
        (
            "task_runner.execution.max_iterations".to_string(),
            json!(300),
        ),
    ]);

    assert!(migrate_agent_iteration_values(&mut values, true));
    assert_eq!(
        values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(700))
    );
    assert!(!values.contains_key("chatos.ai.max_iterations"));
    assert!(!values.contains_key("task_runner.execution.max_iterations"));
}

#[test]
fn explicit_shared_agent_value_wins_over_legacy_values() {
    let mut values = BTreeMap::from([
        (
            chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(900),
        ),
        ("chatos.ai.max_iterations".to_string(), json!(700)),
    ]);

    assert!(migrate_agent_iteration_values(&mut values, true));
    assert_eq!(
        values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(900))
    );
}

#[test]
fn empty_draft_does_not_gain_an_unrequested_change() {
    let mut values = BTreeMap::new();
    assert!(!migrate_agent_iteration_values(&mut values, false));
    assert!(values.is_empty());
}

#[test]
fn audit_keys_replace_legacy_agent_keys_once() {
    let mut keys = vec![
        "chatos.ai.max_iterations".to_string(),
        "task_runner.execution.max_iterations".to_string(),
        "shared.logging.level".to_string(),
    ];

    assert!(migrate_agent_iteration_changed_keys(&mut keys));
    assert_eq!(
        keys.iter()
            .filter(|key| *key == chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
            .count(),
        1
    );
    assert!(!keys
        .iter()
        .any(|key| LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str())));
}

#[test]
fn task_runner_iteration_inherits_shared_agent_limit_when_missing() {
    let mut values = BTreeMap::from([(
        chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
        json!(600),
    )]);

    assert!(ensure_task_runner_iteration_value(&mut values, json!(500)));
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(600))
    );
}

#[test]
fn task_runner_iteration_keeps_explicit_service_limit() {
    let mut values = BTreeMap::from([
        (
            chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(600),
        ),
        (
            TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(900),
        ),
    ]);

    assert!(!ensure_task_runner_iteration_value(&mut values, json!(500)));
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(900))
    );
}

#[test]
fn task_runner_execution_environment_mode_is_backfilled_when_missing() {
    let mut values = BTreeMap::new();

    assert!(ensure_task_runner_execution_environment_mode_value(
        &mut values,
        json!("cloud")
    ));
    assert_eq!(
        values.get(TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY),
        Some(&json!("cloud"))
    );
}

#[test]
fn task_runner_execution_environment_mode_keeps_explicit_service_value() {
    let mut values = BTreeMap::from([(
        TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY.to_string(),
        json!("local"),
    )]);

    assert!(!ensure_task_runner_execution_environment_mode_value(
        &mut values,
        json!("cloud")
    ));
    assert_eq!(
        values.get(TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY),
        Some(&json!("local"))
    );
}

#[test]
fn task_runner_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = task_runner_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
        json!(600),
    )]);

    let changed_keys = ensure_task_runner_runtime_values(&mut values, &defaults);

    assert!(!changed_keys.is_empty());
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing Task Runner config key {key}"
        );
    }
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(600))
    );
    assert!(changed_keys.contains(&TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY.to_string()));
}

#[test]
fn chatos_local_project_creation_backfill_uses_catalog_default() {
    let definitions = builtin_definitions();
    let default_value = chatos_local_project_creation_default_value(&definitions)
        .expect("local project creation default");
    let mut values = BTreeMap::new();

    assert_eq!(default_value, json!(false));
    assert!(ensure_chatos_local_project_creation_value(
        &mut values,
        default_value
    ));
    assert_eq!(
        values.get(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY),
        Some(&json!(false))
    );
}

#[test]
fn chatos_local_project_creation_backfill_keeps_explicit_value() {
    let mut values = BTreeMap::from([(
        CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY.to_string(),
        json!(true),
    )]);

    assert!(!ensure_chatos_local_project_creation_value(
        &mut values,
        json!(false)
    ));
    assert_eq!(
        values.get(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY),
        Some(&json!(true))
    );
}

#[test]
fn chatos_snapshot_exposes_local_project_creation_value_and_legacy_env() {
    let definitions = builtin_definitions();
    for enabled in [false, true] {
        let values = BTreeMap::from([(
            CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY.to_string(),
            json!(enabled),
        )]);
        let snapshot = build_snapshot("local", "chatos-backend", 1, &definitions, &values)
            .expect("ChatOS snapshot");

        assert_eq!(
            snapshot
                .values
                .get(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY),
            Some(&json!(enabled))
        );
        assert_eq!(
            snapshot.env.get("LOCAL_PROJECT_CREATION_ENABLED"),
            Some(&enabled.to_string())
        );
    }
}

#[test]
fn non_chatos_snapshot_does_not_gain_local_project_creation_value() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([(
        CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY.to_string(),
        json!(true),
    )]);
    let snapshot = build_snapshot("local", "task-runner", 1, &definitions, &values)
        .expect("Task Runner snapshot");

    assert!(!snapshot
        .values
        .contains_key(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY));
    assert!(!snapshot.env.contains_key("LOCAL_PROJECT_CREATION_ENABLED"));
}

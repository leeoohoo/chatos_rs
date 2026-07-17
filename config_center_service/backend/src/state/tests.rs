// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::*;

use super::*;

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

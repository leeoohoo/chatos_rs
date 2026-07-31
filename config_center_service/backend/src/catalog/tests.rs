// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn catalog_exposes_shared_and_task_runner_iteration_limits() {
    let definitions = builtin_definitions();
    let iteration_definitions = definitions
        .iter()
        .filter(|definition| definition.key.contains("max_iterations"))
        .collect::<Vec<_>>();

    assert_eq!(iteration_definitions.len(), 2);
    let shared = iteration_definitions
        .iter()
        .find(|definition| definition.key == AGENT_MAX_ITERATIONS_CONFIG_KEY)
        .expect("shared agent iteration definition");
    assert_eq!(shared.scope, "shared");
    assert_eq!(shared.service_name, None);
    assert_eq!(shared.default_value, json!(DEFAULT_AGENT_MAX_ITERATIONS));
    assert!(shared.env_aliases.is_empty());

    let task_runner = iteration_definitions
        .iter()
        .find(|definition| definition.key == TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY)
        .expect("task runner iteration definition");
    assert_eq!(task_runner.scope, "service");
    assert_eq!(task_runner.service_name.as_deref(), Some("task-runner"));
    assert_eq!(
        task_runner.default_value,
        json!(DEFAULT_AGENT_MAX_ITERATIONS)
    );
    assert_eq!(task_runner.max, Some(5000));
    assert!(task_runner.env_aliases.is_empty());
}

#[test]
fn catalog_exposes_task_runner_runtime_controls_without_env_overrides() {
    let definitions = builtin_definitions();
    for key in [
        TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY,
        TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY,
        TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY,
        TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY,
        TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY,
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("task-runner"));
        assert!(
            definition.env_aliases.is_empty(),
            "{key} must be managed from configuration-center values, not env aliases"
        );
    }

    let environment_mode = definitions
        .iter()
        .find(|definition| definition.key == TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY)
        .expect("task runner execution environment mode definition");
    assert_eq!(environment_mode.value_type, "enum");
    assert_eq!(
        environment_mode.default_value,
        json!(default_task_runner_execution_environment_mode())
    );
    assert_eq!(environment_mode.enum_options, vec!["local", "cloud"]);
}

#[test]
fn catalog_exposes_shared_memory_policies_for_server_and_client() {
    let definitions = builtin_definitions();
    let memory_definitions = definitions
        .iter()
        .filter(|definition| definition.key.starts_with("memory_engine.policy."))
        .collect::<Vec<_>>();

    assert!(!memory_definitions.is_empty());
    assert!(memory_definitions
        .iter()
        .all(|definition| definition.scope == "shared"));
    assert!(memory_definitions
        .iter()
        .all(|definition| !definition.key.ends_with("model_profile_id")));
    assert!(memory_definitions.iter().any(|definition| {
        definition.key == "memory_engine.policy.rollup.keep_level0_count"
            && definition.default_value == json!(5)
    }));
    assert!(memory_definitions.iter().any(|definition| {
        definition.key == "memory_engine.policy.thread_repair.token_limit"
            && definition.default_value == json!(200000)
    }));
}

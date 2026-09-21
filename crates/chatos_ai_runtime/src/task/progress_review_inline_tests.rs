#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn review_triggers_after_repeated_read_only_iterations() {
        let progress = TaskExecutionProgressState::default();

        assert!(progress.should_trigger_review(7).is_none());
        let checkpoint = progress
            .should_trigger_review(8)
            .expect("read-only checkpoint");
        assert_eq!(checkpoint.trigger, TaskExecutionReviewTrigger::ReadOnlyLoop);
        assert_eq!(checkpoint.read_only_iterations, 8);
        assert!(progress.should_trigger_review(9).is_none());
        assert!(progress.should_trigger_review(16).is_some());
    }

    #[test]
    fn missing_targeted_reads_trigger_review_without_restricting_tools() {
        let progress = TaskExecutionProgressState::default();
        for iteration in [1, 2] {
            progress.begin_iteration(iteration);
            progress.observe_tool_result(&json!({
                "name": "code_maintainer_read_read_file_raw",
                "success": false,
                "is_error": true,
                "content": "ENOENT: package.json does not exist",
            }));
        }

        let checkpoint = progress
            .should_trigger_review(3)
            .expect("missing-read checkpoint");
        assert_eq!(
            checkpoint.trigger,
            TaskExecutionReviewTrigger::MissingTargetedReads
        );
        assert_eq!(checkpoint.missing_read_failures, 2);
    }

    #[test]
    fn plugin_completion_contract_stays_pending_until_matching_proof_arrives() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "solution_studio_solution_upsert_design",
            "success": true,
            "is_error": false,
            "result": {
                "completionRequirement": {
                    "id": "solution-studio:workspace-1",
                    "verifier": "solution_finalize"
                }
            }
        }));
        assert_eq!(
            progress
                .pending_completion_requirements()
                .get("solution-studio:workspace-1")
                .map(String::as_str),
            Some("solution_finalize")
        );
        assert!(progress.confirmed_acceptance_tools().is_empty());

        progress.observe_tool_result(&json!({
            "name": "solution_studio_solution_finalize",
            "success": true,
            "is_error": false,
            "result": {
                "completionProof": {
                    "id": "solution-studio:workspace-1",
                    "verifier": "solution_finalize",
                    "revision": 3
                }
            }
        }));
        assert!(progress.pending_completion_requirements().is_empty());
        assert_eq!(
            progress.confirmed_acceptance_tools(),
            ["solution_studio_solution_finalize"]
        );
    }

    #[test]
    fn successful_source_write_resets_missing_read_budget() {
        let progress = TaskExecutionProgressState::default();
        for iteration in [1, 2] {
            progress.begin_iteration(iteration);
            progress.observe_tool_result(&json!({
                "name": "code_maintainer_read_read_file",
                "success": false,
                "is_error": true,
                "content": "README.md not found",
            }));
        }
        assert!(progress.should_trigger_review(3).is_some());

        progress.begin_iteration(4);
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {
                "committed_paths": [{ "path": "src/lib.rs" }],
            },
        }));

        assert!(progress.should_trigger_review(5).is_none());
    }

    #[test]
    fn placeholder_progress_write_triggers_review_and_is_not_meaningful_progress() {
        let payload = json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {
                "committed_paths": [{ "path": "TASK_RUNNER_PROGRESS_NOTE.md" }],
            },
        });
        assert!(!tool_result_is_meaningful_engineering_action(&payload));
        assert!(tool_result_is_placeholder_progress_write(&payload));

        let progress = TaskExecutionProgressState::default();
        progress.begin_iteration(4);
        progress.observe_tool_result(&payload);
        let checkpoint = progress
            .should_trigger_review(5)
            .expect("placeholder checkpoint");
        assert_eq!(
            checkpoint.trigger,
            TaskExecutionReviewTrigger::PlaceholderProgressWrite
        );
    }

    #[test]
    fn missing_targeted_read_detection_supports_harness_prefixed_tools() {
        assert!(tool_result_is_missing_targeted_read(&json!({
            "name": "harness_code_read_file_range",
            "success": false,
            "is_error": true,
            "content": "file not found: src/main.rs",
        })));
        assert!(!tool_result_is_missing_targeted_read(&json!({
            "name": "harness_code_search_text",
            "success": false,
            "is_error": true,
            "content": "not found",
        })));
    }

    #[test]
    fn observation_and_task_bookkeeping_are_not_engineering_progress() {
        for name in [
            "task_runner_update_task",
            "task_run_process_record_process",
            "code_maintainer_read_read_file_raw",
        ] {
            assert!(!tool_result_is_meaningful_engineering_action(&json!({
                "name": name,
                "success": true,
                "is_error": false,
            })));
        }
    }

    #[test]
    fn placeholder_paths_are_rejected_but_source_writes_are_meaningful() {
        for path in [
            ".chatos/tmp/inspection-unlock.txt",
            "mdm-service/.progress-guard-placeholder",
            "UNBLOCK.md",
            "src/probe_progress_guard.py",
            "TASK_RUNNER_TEMP_RESTORE.txt",
            "task-runner-temp-unlock.txt",
            "ENABLE_TOOLS_AFTER_WRITE.md",
            "docs/oms-order-entry-task-runner-notes.md",
            "docs/task_runner_execution_notes.md",
        ] {
            let payload = json!({
                "name": "code_maintainer_write_commit_edit_session",
                "success": true,
                "is_error": false,
                "result": { "committed_paths": [{ "path": path }] },
            });
            assert!(!tool_result_is_meaningful_engineering_action(&payload));
            assert!(tool_result_is_placeholder_progress_write(&payload));
        }

        assert!(tool_result_is_meaningful_engineering_action(&json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "committed_paths": [{ "path": "src/lib.rs" }],
            })).expect("content"),
        })));
    }

    #[test]
    fn targeted_test_command_is_meaningful_progress() {
        assert!(tool_result_is_meaningful_engineering_action(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "python -m unittest discover -s tests -v",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        })));
    }

    #[test]
    fn terminal_file_overwrite_is_not_meaningful_engineering_progress() {
        assert!(!tool_result_is_meaningful_engineering_action(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "python3 -c \"from pathlib import Path; Path('a').write_text('x')\"",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        })));
    }

    #[test]
    fn successful_file_tools_build_a_bounded_confirmed_path_index() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_search_text",
            "success": true,
            "is_error": false,
            "result": {
                "matches": [
                    { "path": "src/domain/index.ts", "line": 4 },
                    { "path": "src/domain/index.ts", "line": 9 },
                    { "path": "../outside.rs", "line": 1 },
                    { "path": "/absolute.rs", "line": 1 },
                    { "path": "target/debug/generated.rs", "line": 1 },
                    { "path": "node_modules/pkg/index.js", "line": 1 }
                ]
            }
        }));
        progress.observe_tool_result(&json!({
            "name": "harness_code_commit_edit_session",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "changed_paths": ["src/domain/index.ts", "src/domain/domain.test.ts"]
            })).expect("content")
        }));

        assert_eq!(
            progress.confirmed_project_paths(),
            vec![
                "src/domain/domain.test.ts".to_string(),
                "src/domain/index.ts".to_string(),
            ]
        );
    }

    #[test]
    fn confirmed_project_path_index_stops_at_managed_capacity() {
        let progress = TaskExecutionProgressState::default();
        let entries = (0..MAX_CONFIRMED_PROJECT_PATHS + 16)
            .map(|index| json!({ "path": format!("src/module_{index}.rs") }))
            .collect::<Vec<_>>();

        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_list_dir",
            "success": true,
            "is_error": false,
            "result": { "entries": entries }
        }));

        assert_eq!(
            progress.confirmed_project_paths().len(),
            MAX_CONFIRMED_PROJECT_PATHS
        );
    }

    #[test]
    fn failed_file_tools_do_not_confirm_paths() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": false,
            "is_error": true,
            "content": "not found",
            "result": { "path": "src/missing.rs" }
        }));

        assert!(progress.confirmed_project_paths().is_empty());
    }

    #[test]
    fn repeated_validation_only_counts_once_without_a_new_mutation() {
        let progress = TaskExecutionProgressState::default();
        let validation = json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "npm run build",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&validation);
        progress.begin_iteration(5);
        progress.observe_tool_result(&validation);

        assert!(progress.should_trigger_review(8).is_none());
        let checkpoint = progress
            .should_trigger_review(9)
            .expect("repeated validation must not reset progress");
        assert_eq!(checkpoint.read_only_iterations, 8);
    }

    #[test]
    fn all_successful_validation_commands_are_kept_for_acceptance_evidence() {
        let progress = TaskExecutionProgressState::default();
        for command in ["npm test", "npm run build"] {
            progress.observe_tool_result(&json!({
                "name": "terminal_controller_execute_command",
                "success": true,
                "is_error": false,
                "content": serde_json::to_string(&json!({
                    "common": command,
                    "exit_code": 0,
                })).expect("content"),
                "result": { "exit_code": 0 },
            }));
        }

        assert_eq!(
            progress.confirmed_validation_commands(),
            vec!["npm run build".to_string(), "npm test".to_string()]
        );
    }

    #[test]
    fn async_maven_validation_is_recorded_after_successful_wait() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "mvn clean verify",
                "busy": true,
                "process_id": "local-proc-1",
            })).expect("content"),
            "result": {
                "busy": true,
                "process_id": "local-proc-1",
            },
        }));

        assert!(progress.confirmed_validation_commands().is_empty());

        let restored = TaskExecutionProgressState::default();
        restored.restore_snapshot(&progress.snapshot());

        restored.observe_tool_result(&json!({
            "name": "terminal_controller_process_wait",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "busy": false,
                "process_id": "local-proc-1",
                "exit_code": 0,
                "output": "BUILD SUCCESS",
            })).expect("content"),
            "result": {
                "busy": false,
                "process_id": "local-proc-1",
                "exit_code": 0,
            },
        }));

        assert_eq!(
            restored.confirmed_validation_commands(),
            ["mvn clean verify"]
        );
    }

    #[test]
    fn failed_async_validation_is_not_recorded() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "result": {
                "common": "./gradlew check",
                "busy": true,
                "process_id": "local-proc-2",
            },
        }));
        progress.observe_tool_result(&json!({
            "name": "terminal_controller_process_poll",
            "success": true,
            "is_error": false,
            "result": {
                "busy": false,
                "process_id": "local-proc-2",
                "exit_code": 1,
            },
        }));

        assert!(progress.confirmed_validation_commands().is_empty());
    }

    #[test]
    fn common_node_dependency_and_typecheck_commands_are_kept_for_acceptance_evidence() {
        let progress = TaskExecutionProgressState::default();
        for command in [
            "npm install --package-lock-only --ignore-scripts --registry=https://registry.npmjs.org",
            "npm ci --ignore-scripts --registry=https://registry.npmjs.org",
            "npm run typecheck",
            "npm audit --audit-level=high --json --registry=https://registry.npmjs.org",
        ] {
            progress.observe_tool_result(&json!({
                "name": "terminal_controller_execute_command",
                "success": true,
                "is_error": false,
                "content": serde_json::to_string(&json!({
                    "common": command,
                    "exit_code": 0,
                })).expect("content"),
                "result": { "exit_code": 0 },
            }));
        }

        assert_eq!(
            progress.confirmed_validation_commands(),
            vec![
                "npm audit --audit-level=high --json --registry=https://registry.npmjs.org".to_string(),
                "npm ci --ignore-scripts --registry=https://registry.npmjs.org".to_string(),
                "npm install --package-lock-only --ignore-scripts --registry=https://registry.npmjs.org".to_string(),
                "npm run typecheck".to_string(),
            ]
        );
        assert_eq!(
            progress.confirmed_project_paths(),
            vec!["package-lock.json".to_string(), "package.json".to_string()]
        );
    }

    #[test]
    fn nested_structured_validation_result_is_kept_for_acceptance_evidence() {
        let progress = TaskExecutionProgressState::default();
        progress.observe_tool_result(&json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "_structured_result": {
                    "_structured_result": {
                        "common": "cargo test -p example",
                        "exit_code": 0
                    }
                }
            })).expect("content"),
            "result": {
                "_structured_result": {
                    "_structured_result": {
                        "exit_code": 0
                    }
                }
            },
        }));

        assert_eq!(
            progress.confirmed_validation_commands(),
            vec!["cargo test -p example".to_string()]
        );
    }

    #[test]
    fn project_mutation_allows_one_new_validation_to_count_as_progress() {
        let progress = TaskExecutionProgressState::default();
        let validation = json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "cargo test -p example",
                "exit_code": 0,
            })).expect("content"),
            "result": { "exit_code": 0 },
        });
        let mutation = json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {
                "committed_paths": [{ "path": "src/lib.rs" }],
            },
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&validation);
        progress.begin_iteration(8);
        progress.observe_tool_result(&mutation);
        progress.begin_iteration(12);
        progress.observe_tool_result(&validation);

        assert!(progress.should_trigger_review(19).is_none());
        assert!(progress.should_trigger_review(20).is_some());
    }

    #[test]
    fn repeated_checkpoints_track_reviews_since_last_progress() {
        let progress = TaskExecutionProgressState::default();

        let first = progress.should_trigger_review(8).expect("first checkpoint");
        let second = progress
            .should_trigger_review(16)
            .expect("second checkpoint");
        let third = progress
            .should_trigger_review(24)
            .expect("third checkpoint");

        assert_eq!(first.checkpoints_since_action, 1);
        assert_eq!(second.checkpoints_since_action, 2);
        assert_eq!(third.checkpoints_since_action, 3);
    }

    #[test]
    fn failed_validation_command_is_not_progress() {
        let progress = TaskExecutionProgressState::default();
        let failed_build = json!({
            "name": "terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "common": "npm run build",
                "exit_code": 127,
            })).expect("content"),
            "result": {
                "common": "npm run build",
                "exit_code": 127,
            },
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&failed_build);

        let checkpoint = progress
            .should_trigger_review(8)
            .expect("failed build must not reset review progress");
        assert_eq!(checkpoint.read_only_iterations, 8);
    }

    #[test]
    fn process_input_is_not_validation_progress() {
        let progress = TaskExecutionProgressState::default();
        let process_write = json!({
            "name": "terminal_controller_process_write",
            "success": true,
            "is_error": false,
            "content": "submitted",
        });

        progress.begin_iteration(1);
        progress.observe_tool_result(&process_write);

        assert!(progress.should_trigger_review(8).is_some());
    }

    #[test]
    fn stale_session_write_failure_triggers_actionable_review() {
        let progress = TaskExecutionProgressState::default();
        let stale_patch = json!({
            "name": "code_maintainer_write_stage_edit_batch",
            "success": false,
            "is_error": true,
            "content": "Patch context not found in file. Patch context is stale.",
        });

        progress.begin_iteration(3);
        progress.observe_tool_result(&stale_patch);

        let checkpoint = progress
            .should_trigger_review(4)
            .expect("stale patch failure must trigger review");
        assert_eq!(
            checkpoint.trigger,
            TaskExecutionReviewTrigger::StaleProjectWrite
        );
    }
}

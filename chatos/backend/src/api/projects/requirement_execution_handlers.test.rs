// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::projects::requirement_execution_handlers::rerun_support::execution_batch_has_started_active_tasks;
    use chatos_project_execution::RequirementPlanItem;

    #[test]
    fn stop_request_accepts_discard_tasks_in_both_naming_styles() {
        let snake: StopRequirementExecutionRequest = serde_json::from_value(json!({
            "execution_group_id": "group-1",
            "conversation_id": "session-1",
            "discard_tasks": true
        }))
        .expect("snake case stop request");
        assert!(snake.discard_tasks);

        let camel: StopRequirementExecutionRequest = serde_json::from_value(json!({
            "executionGroupId": "group-1",
            "conversationId": "session-1",
            "discardTasks": true
        }))
        .expect("camel case stop request");
        assert!(camel.discard_tasks);
    }

    #[test]
    fn execute_request_accepts_selected_model_in_snake_case() {
        let request: ExecuteRequirementRequest = serde_json::from_value(serde_json::json!({
            "model_config_id": "model-selected",
            "include_prerequisite_dependents": true,
            "planning_feedback": "先补测试"
        }))
        .expect("request should deserialize");

        assert_eq!(request.model_config_id.as_deref(), Some("model-selected"));
        assert!(request.include_prerequisite_dependents);
        assert_eq!(request.planning_feedback.as_deref(), Some("先补测试"));
    }

    #[test]
    fn execute_request_accepts_selected_model_in_camel_case() {
        let request: ExecuteRequirementRequest = serde_json::from_value(serde_json::json!({
            "modelConfigId": "model-selected"
        }))
        .expect("request should deserialize");

        assert_eq!(request.model_config_id.as_deref(), Some("model-selected"));
    }

    #[test]
    fn new_requirement_planner_turn_does_not_inherit_previous_cancellation() {
        let session_id = "requirement-planner-reset-session";
        abort_registry::clear(session_id);
        abort_registry::reset_turn(session_id, Some("old-turn"));
        assert!(abort_registry::abort_turn(session_id, Some("old-turn")));
        assert!(abort_registry::is_aborted(session_id));

        prepare_requirement_planner_turn(session_id, "new-turn");

        assert!(!abort_registry::is_aborted(session_id));
        let token = abort_registry::abort_token_for_turn(session_id, Some("new-turn"))
            .expect("new planner turn token");
        assert!(!token.is_cancelled());
        assert!(abort_registry::abort_token_for_turn(session_id, Some("old-turn")).is_none());
        abort_registry::clear(session_id);
    }

    #[test]
    fn precise_stop_requires_complete_plan_identity() {
        assert!(precise_cloud_plan_identity(None, None)
            .expect("legacy requirement stop remains supported")
            .is_none());
        assert_eq!(
            precise_cloud_plan_identity(
                Some(" execution-group-1 ".to_string()),
                Some(" session-1 ".to_string()),
            )
            .expect("complete precise stop identity")
            .expect("precise stop"),
            ("session-1".to_string(), "execution-group-1".to_string())
        );
        assert!(precise_cloud_plan_identity(Some("execution-group-1".to_string()), None).is_err());
        assert!(precise_cloud_plan_identity(None, Some("session-1".to_string())).is_err());
    }

    #[test]
    fn latest_execution_message_selection_does_not_depend_on_created_task_links() {
        let mut older = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "older".to_string(),
        );
        older.id = "group-old".to_string();
        older.created_at = "2026-07-23T05:00:00Z".to_string();
        older.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": { "overall_status": "stopped" }
        }));
        let mut newer = older.clone();
        newer.id = "group-new".to_string();
        newer.created_at = "2026-07-23T06:00:00Z".to_string();
        newer.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": { "overall_status": "failed" }
        }));

        assert!(cloud_execution_message_matches_scope(
            &newer,
            "project-1",
            "requirement-1",
        ));
        assert!(cloud_execution_message_is_newer(&newer, &older));
        assert_eq!(execution_message_status(&newer), "failed");
    }

    #[test]
    fn stopped_execution_message_marker_recovers_status_after_late_failure_overwrite() {
        let mut message = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execution".to_string(),
        );
        message.id = "group-stopped".to_string();
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "failed",
                "confirmation_status": "failed",
                "stopped_at": "2026-07-29T09:00:00Z",
                "stopped_task_ids": ["task-1"]
            }
        }));

        assert_eq!(execution_message_status(&message), "stopped");
    }

    #[test]
    fn cancelled_execution_message_is_terminal_for_replacement() {
        let mut message = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execution".to_string(),
        );
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "cancelled",
                "confirmation_status": "cancelled"
            }
        }));

        assert_eq!(execution_message_status(&message), "cancelled");
        assert!(execution_message_is_stopped_terminal(&message));
    }

    #[test]
    fn failed_confirmation_wins_over_inconsistent_completed_overall_status() {
        let mut message = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execution".to_string(),
        );
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "completed",
                "confirmation_status": "failed",
                "created_task_ids": []
            }
        }));

        assert_eq!(execution_message_status(&message), "failed");
    }

    fn execution_link_with_status(status: &str) -> ExecutionLink {
        ExecutionLink {
            link_id: None,
            work_item_id: "work-item-1".to_string(),
            task_runner_task_id: format!("task-{status}"),
            task_runner_run_id: None,
            task_runner_status: Some(status.to_string()),
            source_session_id: Some("session-1".to_string()),
            source_user_message_id: Some("group-1".to_string()),
        }
    }

    #[test]
    fn cancelled_links_recover_replacement_readiness_when_message_status_is_stale() {
        let mut message = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execution".to_string(),
        );
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "failed",
                "confirmation_status": "failed"
            }
        }));

        assert_eq!(
            resolve_old_cloud_execution_batch_state(
                &message,
                &[
                    execution_link_with_status("succeeded"),
                    execution_link_with_status("cancelled")
                ],
            ),
            OldCloudExecutionBatchState::ReplacementReady
        );
    }

    #[test]
    fn active_links_keep_stopped_batch_in_cancellation_settling() {
        let mut message = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execution".to_string(),
        );
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "stopped",
                "confirmation_status": "stopped"
            }
        }));

        assert_eq!(
            resolve_old_cloud_execution_batch_state(
                &message,
                &[
                    execution_link_with_status("running"),
                    execution_link_with_status("cancelled")
                ],
            ),
            OldCloudExecutionBatchState::CancellationSettling(1)
        );
    }

    #[test]
    fn started_active_batch_cannot_be_implicitly_cancelled_by_replanning() {
        let mut running = execution_link_with_status("running");
        running.task_runner_run_id = Some("run-1".to_string());
        let waiting = execution_link_with_status("ready");

        assert!(execution_batch_has_started_active_tasks(&[running, waiting]));
        assert!(!execution_batch_has_started_active_tasks(&[
            execution_link_with_status("ready"),
            execution_link_with_status("pending"),
        ]));
    }

    #[test]
    fn failed_links_without_stop_intent_are_not_replacement_ready() {
        let mut message = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execution".to_string(),
        );
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "failed",
                "confirmation_status": "failed"
            }
        }));

        assert_eq!(
            resolve_old_cloud_execution_batch_state(
                &message,
                &[execution_link_with_status("failed")],
            ),
            OldCloudExecutionBatchState::NotStopped
        );
    }

    #[test]
    fn planner_prompt_requires_task_creation_for_planning_work_items() {
        let requirement = RequirementPlanItem {
            id: "requirement-1".to_string(),
            title: "Plan migration".to_string(),
            status: "approved".to_string(),
            parent_requirement_id: None,
        };
        let work_item = WorkItemPlanItem {
            id: "project-task-1".to_string(),
            requirement_id: requirement.id.clone(),
            title: "Create migration plan".to_string(),
            description: Some("Structured planning details".to_string()),
            status: "ready".to_string(),
            priority: 1,
            tags: vec!["planning".to_string()],
            is_planning_task: true,
        };
        let prompt = build_requirement_execution_planner_prompt(
            "project-1",
            &requirement,
            std::slice::from_ref(&requirement),
            &BTreeSet::from([requirement.id.clone()]),
            std::slice::from_ref(&work_item),
            std::slice::from_ref(&work_item),
            std::slice::from_ref(&work_item.id),
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some("model-selected"),
            None,
        )
        .expect("planner prompt");

        assert!(prompt.contains("must_call_tool"));
        assert!(prompt.contains("create_project_execution_tasks"));
        assert!(prompt.contains("project_task_001"));
        assert!(!prompt.contains("project-task-1"));
        assert!(prompt.contains("model-selected"));
        assert!(!prompt.contains("execution_plane"));
        assert!(!prompt.contains("local_connector"));
        assert!(prompt.contains("运行路由由系统自动完成"));
    }

    #[test]
    fn requirement_execution_user_message_is_concise_and_hides_internal_contract() {
        let requirement = RequirementPlanItem {
            id: "requirement-1".to_string(),
            title: "JDK 21 upgrade".to_string(),
            status: "approved".to_string(),
            parent_requirement_id: None,
        };
        let work_items = (1..=4)
            .map(|index| WorkItemPlanItem {
                id: format!("project-task-{index}"),
                requirement_id: requirement.id.clone(),
                title: format!("Migration task {index}"),
                description: None,
                status: "ready".to_string(),
                priority: index,
                tags: Vec::new(),
                is_planning_task: false,
            })
            .collect::<Vec<_>>();

        let content = build_requirement_execution_user_message(&requirement, &work_items);

        assert!(content.contains("执行需求「JDK 21 upgrade」的 4 个关联任务"));
        assert!(content.contains("Migration task 1"));
        assert!(content.contains("另有 1 个关联任务"));
        assert!(!content.contains("create_project_execution_tasks"));
        assert!(!content.contains("execution_contract"));
        assert!(!content.contains("project-task-1"));
    }

    #[test]
    fn partial_planner_coverage_is_reported_as_failure() {
        let selected = vec![
            WorkItemPlanItem {
                id: "task-1".to_string(),
                requirement_id: "requirement-1".to_string(),
                title: "实现接口".to_string(),
                description: None,
                status: "ready".to_string(),
                priority: 1,
                tags: Vec::new(),
                is_planning_task: false,
            },
            WorkItemPlanItem {
                id: "task-2".to_string(),
                requirement_id: "requirement-1".to_string(),
                title: "补充测试".to_string(),
                description: None,
                status: "ready".to_string(),
                priority: 1,
                tags: Vec::new(),
                is_planning_task: false,
            },
        ];
        let message = build_planner_coverage_failure_message(
            selected.as_slice(),
            &BTreeSet::from(["task-1".to_string()]),
        );
        assert!(message.contains("补充测试"));
        assert!(message.contains("未完整覆盖"));
    }

    #[test]
    fn replanning_cleanup_keeps_completed_old_tasks_in_the_retirement_scope() {
        let selected = vec![WorkItemPlanItem {
            id: "task-new".to_string(),
            requirement_id: "requirement-1".to_string(),
            title: "继续实现".to_string(),
            description: None,
            status: "ready".to_string(),
            priority: 1,
            tags: Vec::new(),
            is_planning_task: false,
        }];
        let replaced = vec![
            selected[0].clone(),
            WorkItemPlanItem {
                id: "task-already-completed".to_string(),
                requirement_id: "requirement-1".to_string(),
                title: "旧批次已完成任务".to_string(),
                description: None,
                status: "done".to_string(),
                priority: 1,
                tags: Vec::new(),
                is_planning_task: false,
            },
        ];

        let scope = replacement_link_scope(selected.as_slice(), replaced.as_slice());

        assert_eq!(
            scope
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["task-new", "task-already-completed"]
        );
    }

    #[test]
    fn confirmation_uses_the_exact_project_task_scope_from_the_planner_message() {
        let metadata = json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1",
                "project_task_ids": ["task-1", "task-2"]
            }
        });
        let expected =
            expected_execution_project_task_ids(Some(&metadata), "project-1", "requirement-1")
                .expect("read original cloud execution scope");
        assert_eq!(
            expected,
            BTreeSet::from(["task-1".to_string(), "task-2".to_string()])
        );

        assert!(expected_execution_project_task_ids(
            Some(&metadata),
            "another-project",
            "requirement-1",
        )
        .is_err());
    }

    #[test]
    fn existing_graph_scope_expands_when_actual_dag_contains_planner_scope() {
        let expected = BTreeSet::from(["task-1".to_string(), "task-2".to_string()]);
        let actual = BTreeSet::from([
            "task-1".to_string(),
            "task-2".to_string(),
            "task-extra".to_string(),
        ]);

        assert_eq!(
            expand_project_task_scope_to_actual_graph(&expected, &actual),
            actual
        );
    }

    #[test]
    fn existing_graph_scope_does_not_hide_missing_planner_tasks() {
        let expected = BTreeSet::from(["task-1".to_string(), "task-2".to_string()]);
        let actual = BTreeSet::from(["task-1".to_string(), "task-extra".to_string()]);

        assert_eq!(
            expand_project_task_scope_to_actual_graph(&expected, &actual),
            expected
        );
    }

    #[test]
    fn rerun_clone_validation_allows_multiple_dag_nodes_per_project_task() {
        let expected = BTreeSet::from(["task-1".to_string(), "task-2".to_string()]);
        let mapped = BTreeSet::from(["task-1".to_string(), "task-2".to_string()]);

        assert!(validate_rerun_cloned_project_task_scope(&expected, &mapped, 4).is_ok());
    }

    #[test]
    fn rerun_clone_validation_rejects_project_task_scope_mismatch() {
        let expected = BTreeSet::from(["task-1".to_string(), "task-2".to_string()]);
        let mapped = BTreeSet::from([
            "task-1".to_string(),
            "task-2".to_string(),
            "task-extra".to_string(),
        ]);

        let error = validate_rerun_cloned_project_task_scope(&expected, &mapped, 4)
            .expect_err("unexpected project task should fail");

        assert!(error.contains("project task scope mismatch"));
        assert!(error.contains("unexpected=[task-extra]"));
        assert!(error.contains("cloned_dag_nodes=4"));
    }

    fn planner_message_created_at(
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> crate::models::message::Message {
        let mut message = crate::models::message::Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "plan".to_string(),
        );
        message.created_at = created_at.to_rfc3339();
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1",
                "project_task_ids": ["task-1"]
            },
            "task_runner_async": {
                "overall_status": "planning",
                "confirmation_status": "planning"
            }
        }));
        message
    }

    #[test]
    fn planning_message_without_tasks_becomes_stale_after_timeout() {
        let now = chrono::Utc::now();
        let message = planner_message_created_at(
            now - chrono::Duration::seconds(STALE_PLANNER_NO_TASK_TIMEOUT_SECONDS + 1),
        );

        assert!(cloud_execution_planner_message_is_stale(
            &message, false, now
        ));
    }

    #[test]
    fn planning_message_with_tasks_is_not_marked_stale() {
        let now = chrono::Utc::now();
        let message = planner_message_created_at(
            now - chrono::Duration::seconds(STALE_PLANNER_NO_TASK_TIMEOUT_SECONDS + 1),
        );

        assert!(!cloud_execution_planner_message_is_stale(
            &message, true, now
        ));
    }

    #[test]
    fn recent_planning_message_is_not_marked_stale() {
        let now = chrono::Utc::now();
        let message = planner_message_created_at(
            now - chrono::Duration::seconds(STALE_PLANNER_NO_TASK_TIMEOUT_SECONDS - 1),
        );

        assert!(!cloud_execution_planner_message_is_stale(
            &message, false, now
        ));
    }
}

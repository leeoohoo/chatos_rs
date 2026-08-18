// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn requirement(id: &str, parent: Option<&str>, status: &str) -> RequirementPlanItem {
    RequirementPlanItem {
        id: id.to_string(),
        title: id.to_string(),
        status: status.to_string(),
        parent_requirement_id: parent.map(ToOwned::to_owned),
    }
}

fn work_item(id: &str, requirement_id: &str, status: &str) -> WorkItemPlanItem {
    WorkItemPlanItem {
        id: id.to_string(),
        requirement_id: requirement_id.to_string(),
        title: id.to_string(),
        description: None,
        status: status.to_string(),
        priority: 0,
        tags: Vec::new(),
        is_planning_task: false,
    }
}

#[test]
fn scope_is_shared_across_execution_planes() {
    let requirements = vec![
        requirement("root", None, "approved"),
        requirement("dependent", None, "approved"),
        requirement("prerequisite", None, "approved"),
        requirement("completed", None, "done"),
    ];
    let dependencies = BTreeMap::from([
        (
            "dependent".to_string(),
            vec!["root".to_string(), "prerequisite".to_string()],
        ),
        ("root".to_string(), vec!["completed".to_string()]),
    ]);
    let scope = collect_requirement_execution_scope(&requirements, "root", &dependencies, false);
    assert_eq!(
        scope,
        BTreeSet::from([
            "dependent".to_string(),
            "prerequisite".to_string(),
            "root".to_string()
        ])
    );
}

#[test]
fn transitive_reduction_keeps_reachability_but_removes_redundant_edges() {
    let node_ids = BTreeSet::from([
        "task-a".to_string(),
        "task-b".to_string(),
        "task-c".to_string(),
    ]);
    let reduction = transitive_reduce_prerequisite_map(
        &node_ids,
        &BTreeMap::from([
            ("task-b".to_string(), vec!["task-a".to_string()]),
            (
                "task-c".to_string(),
                vec!["task-a".to_string(), "task-b".to_string()],
            ),
        ]),
    )
    .expect("valid DAG");

    assert_eq!(
        reduction.dependencies.get("task-c"),
        Some(&vec!["task-b".to_string()])
    );
    assert_eq!(
        reduction.removed_edges,
        vec![DependencyEdge {
            dependent_id: "task-c".to_string(),
            prerequisite_id: "task-a".to_string(),
        }]
    );
}

#[test]
fn transitive_reduction_rejects_cycles_and_unknown_nodes() {
    let node_ids = BTreeSet::from(["task-a".to_string(), "task-b".to_string()]);
    assert!(transitive_reduce_prerequisite_map(
        &node_ids,
        &BTreeMap::from([
            ("task-a".to_string(), vec!["task-b".to_string()]),
            ("task-b".to_string(), vec!["task-a".to_string()]),
        ]),
    )
    .is_err());
    assert!(transitive_reduce_prerequisite_map(
        &node_ids,
        &BTreeMap::from([("task-a".to_string(), vec!["missing".to_string()])]),
    )
    .is_err());
}

#[test]
fn dense_ordered_graph_reduces_to_a_readable_chain() {
    let node_ids = (0..12)
        .map(|index| format!("task-{index:02}"))
        .collect::<BTreeSet<_>>();
    let dependency_map = (0..12)
        .map(|index| {
            (
                format!("task-{index:02}"),
                (0..index)
                    .map(|prerequisite| format!("task-{prerequisite:02}"))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let reduction =
        transitive_reduce_prerequisite_map(&node_ids, &dependency_map).expect("dense ordered DAG");

    assert_eq!(
        reduction.dependencies.values().map(Vec::len).sum::<usize>(),
        11
    );
    assert_eq!(reduction.removed_edges.len(), 55);
    for index in 1..12 {
        assert_eq!(
            reduction.dependencies.get(&format!("task-{index:02}")),
            Some(&vec![format!("task-{:02}", index - 1)])
        );
    }
}

#[test]
fn prompt_keeps_routing_program_owned_and_requires_exact_coverage() {
    let requirement = requirement("req-1", None, "approved");
    let task = WorkItemPlanItem {
        id: "task-1".to_string(),
        requirement_id: requirement.id.clone(),
        title: "实现接口".to_string(),
        description: Some("根据技术文档实现".to_string()),
        status: "ready".to_string(),
        priority: 1,
        tags: Vec::new(),
        is_planning_task: false,
    };
    let prompt = build_requirement_execution_planner_prompt(
        "project-1",
        &requirement,
        std::slice::from_ref(&requirement),
        &BTreeSet::from([requirement.id.clone()]),
        std::slice::from_ref(&task),
        std::slice::from_ref(&task),
        std::slice::from_ref(&task.id),
        &BTreeMap::new(),
        &BTreeMap::new(),
        Some("model-1"),
        Some("先补测试，再改实现"),
    )
    .expect("planner prompt");
    assert!(!prompt.contains("execution_plane"));
    assert!(!prompt.contains("local_connector"));
    assert!(prompt.contains("运行路由由系统自动完成"));
    assert!(prompt.contains("先补测试，再改实现"));
    assert!(prompt.contains("project_task_001"));
    assert!(!prompt.contains("selected_project_task_ids"));
    assert!(prompt.contains("create_project_execution_tasks"));
}

#[test]
fn prompt_separates_pending_context_and_satisfied_prerequisites() {
    let requirement = requirement("req-1", None, "approved");
    let completed = WorkItemPlanItem {
        id: "task-completed".to_string(),
        requirement_id: requirement.id.clone(),
        title: "已完成基础设施".to_string(),
        description: None,
        status: "done".to_string(),
        priority: 1,
        tags: Vec::new(),
        is_planning_task: false,
    };
    let pending = WorkItemPlanItem {
        id: "task-pending".to_string(),
        requirement_id: requirement.id.clone(),
        title: "实现业务逻辑".to_string(),
        description: None,
        status: "ready".to_string(),
        priority: 1,
        tags: Vec::new(),
        is_planning_task: false,
    };
    let prompt = build_requirement_execution_planner_prompt(
        "project-1",
        &requirement,
        std::slice::from_ref(&requirement),
        &BTreeSet::from([requirement.id.clone()]),
        &[completed.clone(), pending.clone()],
        std::slice::from_ref(&pending),
        std::slice::from_ref(&pending.id),
        &BTreeMap::from([(pending.id.clone(), vec![completed.id.clone()])]),
        &BTreeMap::new(),
        None,
        None,
    )
    .expect("completed prerequisite is context, not a blocker");

    assert!(prompt.contains("satisfied_prerequisite_project_tasks"));
    assert!(!prompt.contains("task-completed"));
    assert!(prompt.contains("已完成基础设施"));
    assert!(prompt.contains("pending_prerequisite_project_task_refs"));
    assert!(prompt.contains("context_prerequisite_project_task_refs"));
    assert!(!prompt.contains("\"prerequisite_project_task_ids\""));
}

#[test]
fn plan_identity_is_shared_across_execution_planes() {
    assert_eq!(
        ExecutionPlanIdentity::required(" group-1 ", " session-1 ").expect("complete identity"),
        ExecutionPlanIdentity {
            execution_group_id: "group-1".to_string(),
            conversation_id: "session-1".to_string(),
        }
    );
    assert!(ExecutionPlanIdentity::optional(Some("group-1"), None).is_err());
}

#[test]
fn planning_feedback_history_appends_without_overwriting_legacy_feedback() {
    let legacy = json!({ "planning_feedback": "先使用 PostgreSQL" });
    let history = read_planning_feedback_history(Some(&legacy));
    let history = append_planning_feedback(&history, Some("再增加 review 任务"));

    assert_eq!(
        history,
        vec![
            "先使用 PostgreSQL".to_string(),
            "再增加 review 任务".to_string(),
        ]
    );
    assert_eq!(
        format_planning_feedback_history(&history).as_deref(),
        Some("1. 先使用 PostgreSQL\n2. 再增加 review 任务")
    );
}

#[test]
fn planning_feedback_history_prefers_the_persisted_history_array() {
    let execution = json!({
        "planning_feedback": "最新意见",
        "planning_feedback_history": ["第一条", "最新意见"]
    });

    assert_eq!(
        read_planning_feedback_history(Some(&execution)),
        vec!["第一条".to_string(), "最新意见".to_string()]
    );
}

#[test]
fn exact_scope_validation_reports_missing_and_unexpected_tasks() {
    let error = validate_exact_project_task_scope(
        &BTreeSet::from(["task-a".to_string(), "task-b".to_string()]),
        &BTreeSet::from(["task-a".to_string(), "task-c".to_string()]),
    )
    .expect_err("scope mismatch");
    assert_eq!(error.missing, vec!["task-b"]);
    assert_eq!(error.unexpected, vec!["task-c"]);
}

#[test]
fn local_and_cloud_task_statuses_share_one_semantic_state_machine() {
    assert_eq!(
        classify_execution_task_status("todo"),
        ExecutionTaskState::Planned
    );
    assert_eq!(
        classify_execution_task_status("ready"),
        ExecutionTaskState::Planned
    );
    assert_eq!(
        classify_execution_task_status("doing"),
        ExecutionTaskState::Active
    );
    assert_eq!(
        classify_execution_task_status("running"),
        ExecutionTaskState::Active
    );
    assert!(execution_task_status_blocks_confirmation("blocked"));
    assert!(execution_task_status_is_success("succeeded"));
}

#[test]
fn requirement_execution_recovery_state_is_server_owned() {
    let rerun = requirement_execution_recovery_state(STATUS_STOPPED, 3, true, true, false);
    assert_eq!(rerun.action, RECOVERY_ACTION_RERUN);
    assert_eq!(rerun.reason, "stopped_with_task_graph");
    assert!(rerun.replace_previous_batch);

    let regenerate = requirement_execution_recovery_state(STATUS_STOPPED, 0, false, true, false);
    assert_eq!(regenerate.action, RECOVERY_ACTION_REGENERATE);
    assert_eq!(regenerate.reason, "stopped_without_task_graph");
    assert!(regenerate.replace_previous_batch);

    let discarded = requirement_execution_recovery_state(STATUS_STOPPED, 3, true, true, true);
    assert_eq!(discarded.action, RECOVERY_ACTION_REGENERATE);
    assert_eq!(discarded.reason, "stopped_after_task_discard");
    assert!(discarded.replace_previous_batch);

    let settling = requirement_execution_recovery_state(STATUS_STOPPING, 3, true, true, false);
    assert_eq!(settling.action, RECOVERY_ACTION_NONE);
    assert_eq!(settling.reason, "cancellation_settling");
    assert!(settling.replace_previous_batch);
}

#[test]
fn missing_prerequisite_is_rejected_for_every_execution_plane() {
    let requirements = vec![requirement("root", None, "approved")];
    let error = validate_requirement_prerequisites(
        requirements.as_slice(),
        &BTreeSet::from(["root".to_string()]),
        &BTreeMap::from([("root".to_string(), vec!["missing".to_string()])]),
    )
    .expect_err("missing prerequisite");
    assert!(error.contains("不存在或不可见"));
}

#[test]
fn pending_task_selection_and_requirement_status_scope_are_shared() {
    let tasks = vec![
        WorkItemPlanItem {
            id: "task-ready".to_string(),
            requirement_id: "child".to_string(),
            title: "Ready".to_string(),
            description: None,
            status: "ready".to_string(),
            priority: 1,
            tags: Vec::new(),
            is_planning_task: false,
        },
        WorkItemPlanItem {
            id: "task-done".to_string(),
            requirement_id: "child".to_string(),
            title: "Done".to_string(),
            description: None,
            status: "done".to_string(),
            priority: 2,
            tags: Vec::new(),
            is_planning_task: false,
        },
    ];
    let selected = select_pending_work_items(
        tasks.as_slice(),
        &BTreeSet::from(["root".to_string(), "child".to_string()]),
    );
    assert_eq!(selected.len(), 1);
    assert_eq!(
        executing_requirement_ids("root", selected.as_slice()),
        BTreeSet::from(["root".to_string(), "child".to_string()])
    );
}

#[test]
fn unblocked_selection_keeps_the_runnable_part_of_a_requirement_batch() {
    let tasks = vec![
        work_item("task-ready", "visual", "ready"),
        work_item("task-blocked", "visual", "todo"),
        work_item("external", "chapter-one", "todo"),
    ];
    let selected = select_unblocked_pending_work_items(
        tasks.as_slice(),
        &BTreeSet::from(["visual".to_string()]),
        &BTreeMap::from([("task-blocked".to_string(), vec!["external".to_string()])]),
    )
    .expect("the runnable task should form the current batch");

    assert_eq!(
        selected
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-ready"]
    );
}

#[test]
fn unblocked_selection_cascades_through_tasks_blocked_by_a_filtered_task() {
    let tasks = vec![
        work_item("task-ready", "visual", "ready"),
        work_item("task-blocked", "visual", "todo"),
        work_item("task-downstream", "visual", "todo"),
        work_item("external", "chapter-one", "todo"),
    ];
    let selected = select_unblocked_pending_work_items(
        tasks.as_slice(),
        &BTreeSet::from(["visual".to_string()]),
        &BTreeMap::from([
            ("task-blocked".to_string(), vec!["external".to_string()]),
            (
                "task-downstream".to_string(),
                vec!["task-blocked".to_string()],
            ),
        ]),
    )
    .expect("only the independent task should be selected");

    assert_eq!(
        selected
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-ready"]
    );
}

#[test]
fn unblocked_selection_reports_when_every_pending_task_is_blocked() {
    let tasks = vec![
        work_item("task-blocked", "visual", "todo"),
        work_item("external", "chapter-one", "todo"),
    ];
    let error = select_unblocked_pending_work_items(
        tasks.as_slice(),
        &BTreeSet::from(["visual".to_string()]),
        &BTreeMap::from([("task-blocked".to_string(), vec!["external".to_string()])]),
    )
    .expect_err("the batch has no runnable task");

    assert!(error.contains("没有已解锁"));
    assert!(error.contains("external"));
}

#[test]
fn unblocked_selection_accepts_completed_external_prerequisites_and_later_batches() {
    let mut tasks = vec![
        work_item("task-first", "visual", "ready"),
        work_item("task-later", "visual", "todo"),
        work_item("external", "chapter-one", "todo"),
    ];
    let dependencies = BTreeMap::from([("task-later".to_string(), vec!["external".to_string()])]);
    let first_batch = select_unblocked_pending_work_items(
        tasks.as_slice(),
        &BTreeSet::from(["visual".to_string()]),
        &dependencies,
    )
    .expect("the first invocation selects the independent task");
    assert_eq!(
        first_batch
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-first"]
    );

    tasks[0].status = "done".to_string();
    tasks[2].status = "done".to_string();
    let later_batch = select_unblocked_pending_work_items(
        tasks.as_slice(),
        &BTreeSet::from(["visual".to_string()]),
        &dependencies,
    )
    .expect("a later invocation selects only the remaining task");
    assert_eq!(
        later_batch
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-later"]
    );
}

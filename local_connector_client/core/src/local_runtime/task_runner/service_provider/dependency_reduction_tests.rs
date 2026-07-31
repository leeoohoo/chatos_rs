// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::{decode, reduce_local_project_execution_dependencies, CreateProjectExecutionTasksArgs};

#[test]
fn local_materializer_preserves_removed_hard_edges_as_context() {
    let mut args: CreateProjectExecutionTasksArgs = decode(json!({
        "project_id": "project-1",
        "requirement_id": "requirement-1",
        "tasks": [
            { "client_ref": "a", "project_task_id": "p-a", "title": "A", "objective": "A", "is_planning_task": false },
            { "client_ref": "b", "project_task_id": "p-b", "title": "B", "objective": "B", "is_planning_task": false, "prerequisite_refs": ["a"] },
            { "client_ref": "c", "project_task_id": "p-c", "title": "C", "objective": "C", "is_planning_task": false, "prerequisite_refs": ["a", "b"] }
        ]
    }))
    .expect("local project graph args");

    let diagnostics = reduce_local_project_execution_dependencies(args.tasks.as_mut_slice())
        .expect("valid local graph");

    assert_eq!(diagnostics.submitted_edge_count, 3);
    assert_eq!(diagnostics.persisted_edge_count, 2);
    assert_eq!(args.tasks[2].prerequisite_refs, vec!["b"]);
    assert_eq!(args.tasks[2].context_refs, vec!["a"]);
}

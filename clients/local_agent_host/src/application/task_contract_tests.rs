// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::tests::{invocation, runtime_with_parent};
use super::*;

#[tokio::test]
async fn requirement_quote_must_come_from_the_authoritative_user_message() {
    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let mut call = invocation(json!({
        "requirements": [{
            "requirement_id":"R1",
            "source_message_id":"message-1",
            "source_quote":"invented words",
            "interpreted_requirement":"Return an observable configuration report",
            "requires_change":false
        }],
        "title":"Inspect",
        "objective":"Read configuration",
        "expected_outputs":["[R1] Configuration report"],
        "acceptance_criteria":["[R1] Report cites source evidence"],
        "requires_execution":false,
        "enabled_builtin_kinds":["CodeMaintainerRead"]
    }));
    call.tool_name = CREATE_TASK_TOOL.into();
    let error = preparation_error(executor.execute_tool(&call).await.unwrap());
    assert!(error.contains("exact excerpt"), "{error}");
}

#[tokio::test]
async fn every_change_requirement_needs_an_implementation_owner() {
    let base = json!({
        "requirements": [
            {"requirement_id":"R1","source_message_id":"message-1","source_quote":"plan this","interpreted_requirement":"Remove the obsolete menu entry","requires_change":true},
            {"requirement_id":"R2","source_message_id":"message-1","source_quote":"plan this","interpreted_requirement":"Repair the business initialization behavior","requires_change":true}
        ],
        "graph_goal":"Implement and verify both requested changes",
        "decomposition_rationale":"Implementation and independent verification have different responsibilities",
        "tasks":[
            {"client_ref":"implementation","title":"Implement","objective":"Make the requested changes","expected_outputs":["[R1] Menu change"],"acceptance_criteria":["[R1] Obsolete entry is absent"],"requirement_refs":["R1"],"requires_execution":true,"enabled_builtin_kinds":["CodeMaintainerWrite"]},
            {"client_ref":"verification","title":"Verify","objective":"Verify both outcomes","expected_outputs":["[R1][R2] Verification report"],"acceptance_criteria":["[R1][R2] Both outcomes are checked"],"requirement_refs":["R1","R2"],"requires_execution":false,"enabled_builtin_kinds":["CodeMaintainerRead"]}
        ]
    });
    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let error = preparation_error(
        executor
            .execute_tool(&invocation(base.clone()))
            .await
            .unwrap(),
    );
    assert!(
        error.contains("R2") && error.contains("read-only verification"),
        "{error}"
    );

    let mut repaired = base;
    repaired["tasks"][0]["requirement_refs"] = json!(["R1", "R2"]);
    repaired["tasks"][0]["expected_outputs"] = json!(["[R1][R2] Both implemented changes"]);
    repaired["tasks"][0]["acceptance_criteria"] =
        json!(["[R1][R2] Both requested behaviors are implemented"]);
    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    assert!(matches!(
        executor.execute_tool(&invocation(repaired)).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
}

#[tokio::test]
async fn verification_contract_requires_a_valid_executable_check_graph() {
    let base = json!({
        "requirements":[{"requirement_id":"R1","source_message_id":"message-1","source_quote":"plan this","interpreted_requirement":"Run the project-defined checks for the frozen artifact","requires_change":false}],
        "title":"Verify",
        "objective":"Verify the frozen artifact",
        "expected_outputs":["[R1] Complete verification report"],
        "acceptance_criteria":["[R1] Every project-defined check is reported"],
        "requires_execution":true,
        "enabled_builtin_kinds":["TerminalController"],
        "verification_contract":{
            "subject_identity":"artifact:abc123",
            "workflow_entry":"./scripts/verify",
            "checks":[
                {"check_id":"compile","depends_on":[]},
                {"check_id":"smoke","depends_on":["compile"]}
            ],
            "require_full_rerun":true
        }
    });
    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let mut unknown_dependency = base.clone();
    unknown_dependency["verification_contract"]["checks"][1]["depends_on"] = json!(["missing"]);
    let mut call = invocation(unknown_dependency);
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(
        preparation_error(executor.execute_tool(&call).await.unwrap())
            .contains("unknown verification dependency")
    );

    let mut cycle = base.clone();
    cycle["verification_contract"]["checks"][0]["depends_on"] = json!(["smoke"]);
    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let mut call = invocation(cycle);
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(preparation_error(executor.execute_tool(&call).await.unwrap()).contains("cycle"));

    let mut read_only = base.clone();
    read_only["requires_execution"] = json!(false);
    read_only["enabled_builtin_kinds"] = json!(["CodeMaintainerRead"]);
    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let mut call = invocation(read_only);
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(
        preparation_error(executor.execute_tool(&call).await.unwrap())
            .contains("requires_execution=true")
    );

    let executor = LocalTaskToolExecutor::new(runtime_with_parent().await, "user-1").unwrap();
    let mut call = invocation(base);
    call.tool_name = CREATE_TASK_TOOL.into();
    assert!(matches!(
        executor.execute_tool(&call).await.unwrap(),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
    let graph = executor
        .runtime
        .task_graph_by_id("user-1", "local-task-graph-invocation-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        graph.tasks[0].input["verification_contract"]["subject_identity"],
        "artifact:abc123"
    );
    assert_eq!(
        graph.tasks[0].input["verification_contract"]["checks"][1]["depends_on"][0],
        "compile"
    );
}

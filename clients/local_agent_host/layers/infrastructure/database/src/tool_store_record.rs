// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::{
    LocalAgentToolApprovalStatus, LocalAgentToolExecutionRoute, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome, LocalAgentToolStatus,
};
use serde_json::Value;
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::str::FromStr;

pub(super) async fn fetch_invocation(
    connection: &mut SqliteConnection,
    invocation_id: &str,
) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT invocation_id, run_id, batch_id, call_id, tool_name, execution_route, arguments_json, \
         side_effecting, requires_approval, approval_status, approval_decided_by, \
         approval_reason, approval_decided_at_unix_ms, status, result_json, error_text, version, claim_token, \
         claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms \
         FROM local_agent_tool_invocations WHERE invocation_id = ?",
    )
    .bind(invocation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_invocation)
    .transpose()
}

pub(super) fn decode_invocation(
    row: SqliteRow,
) -> Result<LocalAgentToolInvocationRecord, ClientStorageError> {
    let status: String = row.try_get("status").db()?;
    let approval_status: String = row.try_get("approval_status").db()?;
    let execution_route: String = row.try_get("execution_route").db()?;
    let arguments: String = row.try_get("arguments_json").db()?;
    let result: Option<String> = row.try_get("result_json").db()?;
    Ok(LocalAgentToolInvocationRecord {
        invocation_id: row.try_get("invocation_id").db()?,
        run_id: row.try_get("run_id").db()?,
        batch_id: row.try_get("batch_id").db()?,
        call_id: row.try_get("call_id").db()?,
        tool_name: row.try_get("tool_name").db()?,
        execution_route: LocalAgentToolExecutionRoute::from_str(&execution_route)
            .map_err(ClientStorageError::InvalidState)?,
        arguments: serde_json::from_str(&arguments)?,
        side_effecting: row.try_get("side_effecting").db()?,
        requires_approval: row.try_get("requires_approval").db()?,
        approval_status: LocalAgentToolApprovalStatus::from_str(&approval_status)
            .map_err(ClientStorageError::InvalidState)?,
        approval_decided_by: row.try_get("approval_decided_by").db()?,
        approval_reason: row.try_get("approval_reason").db()?,
        approval_decided_at_unix_ms: row.try_get("approval_decided_at_unix_ms").db()?,
        status: LocalAgentToolStatus::from_str(&status)
            .map_err(ClientStorageError::InvalidState)?,
        result: result
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        error: row.try_get("error_text").db()?,
        version: u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
            ClientStorageError::InvalidState("invalid tool invocation version".to_string())
        })?,
        claim_token: row.try_get("claim_token").db()?,
        claim_until_unix_ms: row.try_get("claim_until_unix_ms").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

pub(super) fn tool_execution_route(
    profile_key: &str,
    tool_name: &str,
) -> LocalAgentToolExecutionRoute {
    const SHARED_TOOLS: &[&str] = &[
        "list_tasks",
        "get_task",
        "create_task",
        "create_tasks_with_prerequisites",
        "cancel_task",
        "wait_for_task_completion",
        "get_task_dependency_graph",
        "resolve_task_followup",
        "ask_user_prompt_key_values",
        "ask_user_prompt_choices",
        "ask_user_prompt_mixed_form",
        "notepad_list_folders",
        "notepad_list_notes",
        "notepad_read_note",
        "notepad_create_note",
        "notepad_update_note",
        "requirement_survey_create",
        "requirement_plan_save",
        "task_run_process_record_process",
        "task_run_process_allocate_service",
        "task_run_process_report_outcome",
        "tool_result_read",
        "tool_result_query",
    ];
    const SKILL_TOOLS: &[&str] = &["agent_skill_activate", "agent_skill_read_resource"];
    if SHARED_TOOLS.contains(&tool_name)
        || (profile_key != "task_execution" && SKILL_TOOLS.contains(&tool_name))
    {
        LocalAgentToolExecutionRoute::Shared
    } else {
        LocalAgentToolExecutionRoute::Platform
    }
}

pub(super) fn outcome_fields(
    outcome: &LocalAgentToolOutcome,
) -> (LocalAgentToolStatus, Option<Value>, Option<&str>) {
    match outcome {
        LocalAgentToolOutcome::Succeeded { output } => {
            (LocalAgentToolStatus::Succeeded, Some(output.clone()), None)
        }
        LocalAgentToolOutcome::Failed { error, detail } => (
            LocalAgentToolStatus::Failed,
            Some(detail.clone()),
            Some(error.as_str()),
        ),
        LocalAgentToolOutcome::NeedsReview { reason, detail } => (
            LocalAgentToolStatus::NeedsReview,
            Some(detail.clone()),
            Some(reason.as_str()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_skill_tools_route_by_run_profile() {
        assert_eq!(
            tool_execution_route("main_chat", "agent_skill_activate"),
            LocalAgentToolExecutionRoute::Shared
        );
        assert_eq!(
            tool_execution_route("task_execution", "agent_skill_activate"),
            LocalAgentToolExecutionRoute::Platform
        );
        assert_eq!(
            tool_execution_route("task_execution", "agent_skill_read_resource"),
            LocalAgentToolExecutionRoute::Platform
        );
        assert_eq!(
            tool_execution_route("main_chat", "read_file"),
            LocalAgentToolExecutionRoute::Platform
        );
    }
}

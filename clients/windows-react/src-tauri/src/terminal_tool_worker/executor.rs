use chatos_local_agent_protocol::{
    LocalAgentToolApprovalStatus, LocalAgentToolClaim, LocalAgentToolExecutionRoute,
    LocalAgentToolOutcome,
};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use crate::{
    host_lifecycle::HostManager,
    platform_execution_context::{
        resolve_execution_directory, resolve_platform_execution_context, resolve_terminal_command,
        resolve_terminal_path, resolve_terminal_purpose,
    },
    terminal_runtime::{ExecuteRequest, TerminalProcessStore, TerminalRuntimeError, TerminalScope},
};

pub(super) async fn execute(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
    processes: &TerminalProcessStore,
) -> LocalAgentToolOutcome {
    match execute_checked(app, owner_user_id, claim, processes).await {
        Ok(output) => LocalAgentToolOutcome::Succeeded { output },
        Err(error) if claim.invocation.side_effecting && !error.known_pre_effect => {
            LocalAgentToolOutcome::NeedsReview {
                reason: safe_error(&error.message),
                detail: error_detail(&claim.invocation.tool_name),
            }
        }
        Err(error) => LocalAgentToolOutcome::Failed {
            error: safe_error(&error.message),
            detail: error_detail(&claim.invocation.tool_name),
        },
    }
}

async fn execute_checked(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
    processes: &TerminalProcessStore,
) -> Result<Value, ExecutionError> {
    let invocation = &claim.invocation;
    if invocation.execution_route != LocalAgentToolExecutionRoute::Platform
        || !super::TERMINAL_TOOL_NAMES.contains(&invocation.tool_name.as_str())
    {
        return Err(ExecutionError::pre_effect(
            "The requested local platform tool is unavailable.",
        ));
    }
    let context = resolve_platform_execution_context(
        app,
        &app.state::<HostManager>(),
        owner_user_id,
        &invocation.run_id,
    )
    .await
    .map_err(|_| ExecutionError::pre_effect("The local platform tool Run context is invalid."))?;
    if !context.authorization.allows_terminal() {
        return Err(ExecutionError::pre_effect(
            "This Task did not select the requested local capability.",
        ));
    }
    if matches!(
        invocation.tool_name.as_str(),
        "execute_command" | "process_write" | "process_kill"
    ) && (!invocation.requires_approval
        || invocation.approval_status != LocalAgentToolApprovalStatus::Approved)
    {
        return Err(ExecutionError::pre_effect(
            "The local terminal change requires approval before execution.",
        ));
    }
    let arguments = invocation
        .arguments
        .as_object()
        .ok_or_else(|| ExecutionError::pre_effect("The local terminal arguments are invalid."))?;
    let scope = TerminalScope {
        run_id: invocation.run_id.clone(),
        workspace_scope_id: context.workspace_scope_id,
        project_root: context.execution_root.clone(),
    };
    match invocation.tool_name.as_str() {
        "execute_command" => {
            let command = resolve_terminal_command(arguments)
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field("command")))?;
            resolve_terminal_purpose(arguments)
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field("purpose")))?;
            let cwd = resolve_execution_directory(
                &context.execution_root,
                resolve_terminal_path(arguments),
            )
            .map_err(ExecutionError::pre_effect)?;
            let background = optional_bool(arguments, "background")?.unwrap_or(false);
            let timeout_ms = command_timeout(arguments)?;
            processes
                .execute(
                    &scope,
                    ExecuteRequest {
                        command: command.to_owned(),
                        cwd,
                        background,
                        timeout_ms,
                    },
                )
                .await
                .map_err(ExecutionError::from)
        }
        "process_poll" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .poll(
                    &scope,
                    id,
                    optional_u64(arguments, "offset")?,
                    optional_usize(arguments, "limit")?.unwrap_or(80),
                )
                .await
                .map_err(ExecutionError::from)
        }
        "process_log" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .log(
                    &scope,
                    id,
                    optional_u64(arguments, "offset")?,
                    optional_usize(arguments, "limit")?.unwrap_or(200),
                )
                .await
                .map_err(ExecutionError::from)
        }
        "process_wait" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .wait(&scope, id, wait_timeout(arguments)?.unwrap_or(30_000))
                .await
                .map_err(ExecutionError::from)
        }
        "process_write" => {
            let id = required_string(arguments, "terminal_id")?;
            let data = arguments
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field("data")))?;
            let submit = optional_bool(arguments, "submit")?.unwrap_or(false);
            processes
                .write(&scope, id, data, submit)
                .await
                .map_err(ExecutionError::from)
        }
        "process_kill" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .kill(&scope, id)
                .await
                .map_err(ExecutionError::from)
        }
        _ => Err(ExecutionError::pre_effect(
            "The requested local platform tool is unavailable.",
        )),
    }
}

fn command_timeout(arguments: &Map<String, Value>) -> Result<Option<u64>, ExecutionError> {
    if let Some(value) = optional_u64(arguments, "timeout_ms")? {
        return Ok(Some(value));
    }
    Ok(optional_u64(arguments, "timeout")?.map(|seconds| seconds.saturating_mul(1_000)))
}

fn wait_timeout(arguments: &Map<String, Value>) -> Result<Option<u64>, ExecutionError> {
    command_timeout(arguments)
}

fn required_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, ExecutionError> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 160)
        .ok_or_else(|| ExecutionError::pre_effect(invalid_field(field)))
}

fn optional_bool(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<bool>, ExecutionError> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field(field)))
        })
        .transpose()
}

fn optional_u64(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<u64>, ExecutionError> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field(field)))
        })
        .transpose()
}

fn optional_usize(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<usize>, ExecutionError> {
    optional_u64(arguments, field)
        .map(|value| value.map(|number| usize::try_from(number).unwrap_or(usize::MAX)))
}

fn invalid_field(field: &str) -> String {
    format!("The local terminal field is invalid: {field}.")
}

fn error_detail(tool_name: &str) -> Value {
    json!({
        "tool_name": tool_name,
        "phase": "windows_terminal_execution"
    })
}

fn safe_error(error: &str) -> String {
    let lowered = error.to_ascii_lowercase();
    if [
        "authorization",
        "api_key",
        "apikey",
        "access_token",
        "password",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        "Local terminal execution failed; sensitive details were hidden.".into()
    } else {
        error.chars().take(1_000).collect()
    }
}

struct ExecutionError {
    message: String,
    known_pre_effect: bool,
}

impl ExecutionError {
    fn pre_effect(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            known_pre_effect: true,
        }
    }
}

impl From<TerminalRuntimeError> for ExecutionError {
    fn from(error: TerminalRuntimeError) -> Self {
        Self {
            message: error.message,
            known_pre_effect: error.known_pre_effect,
        }
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chatos_agent_profiles::{
    TaskRunnerCapabilitySnapshot, TaskRunnerExecutionTool, TaskRunnerProjectSnapshot,
};
use chatos_client_storage::{
    AgentRunStateRecord, ClientStorage, RecordQuery, RecordScope, StorageError, StorageResult,
    StorageTransaction, TaskRecord, TransactionRepositories,
};
use chatos_local_agent_protocol::{FrozenSnapshot, MAX_BOUNDED_JSON_BYTES};
use chatos_local_agent_runtime::{LocalToolInvocation, LocalToolOutcome, LocalToolRuntime};
use chatos_mcp_runtime::{McpExecutor, ToolCallContext, ToolResult};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

const TOOL_RESULT_MAX_CHARS: usize = 24_000;
const REMOTE_EXECUTION_SERVERS: [&str; 2] = ["task_runner_service", "mcp_management"];

/// The exact MCP executor resolved from the plugin releases frozen into a Task.
/// The provider must fail when those releases are no longer locally available;
/// silently resolving a newer installed release is forbidden.
pub struct FrozenMcpExecutor {
    pub plugin_release_snapshot: Value,
    pub executor: Arc<McpExecutor>,
}

#[async_trait]
pub trait FrozenMcpExecutorProvider: Send + Sync {
    async fn resolve(
        &self,
        plugin_release_snapshot: &Value,
        cancellation: CancellationToken,
    ) -> Result<FrozenMcpExecutor, String>;
}

#[derive(Default)]
pub struct RegisteredFrozenMcpExecutorProvider {
    entries: RwLock<Vec<RegisteredFrozenMcpExecutor>>,
}

struct RegisteredFrozenMcpExecutor {
    plugin_release_snapshot: Value,
    executor: Arc<McpExecutor>,
}

impl RegisteredFrozenMcpExecutorProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an already initialized local executor for one exact release
    /// set. Re-registering the same immutable snapshot replaces its executor;
    /// it never aliases or upgrades a different release set.
    pub fn register(
        &self,
        plugin_release_snapshot: Value,
        executor: Arc<McpExecutor>,
    ) -> Result<(), String> {
        if !plugin_release_snapshot.is_object() {
            return Err("plugin release snapshot must be an object".to_string());
        }
        let mut entries = self
            .entries
            .write()
            .map_err(|_| "frozen MCP executor registry lock is poisoned".to_string())?;
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.plugin_release_snapshot == plugin_release_snapshot)
        {
            entry.executor = executor;
        } else {
            entries.push(RegisteredFrozenMcpExecutor {
                plugin_release_snapshot,
                executor,
            });
        }
        Ok(())
    }
}

#[async_trait]
impl FrozenMcpExecutorProvider for RegisteredFrozenMcpExecutorProvider {
    async fn resolve(
        &self,
        plugin_release_snapshot: &Value,
        cancellation: CancellationToken,
    ) -> Result<FrozenMcpExecutor, String> {
        if cancellation.is_cancelled() {
            return Err("local MCP executor resolution was cancelled".to_string());
        }
        let entries = self
            .entries
            .read()
            .map_err(|_| "frozen MCP executor registry lock is poisoned".to_string())?;
        let entry = entries
            .iter()
            .find(|entry| entry.plugin_release_snapshot == *plugin_release_snapshot)
            .ok_or_else(|| {
                "no initialized local MCP executor matches the frozen plugin releases".to_string()
            })?;
        Ok(FrozenMcpExecutor {
            plugin_release_snapshot: entry.plugin_release_snapshot.clone(),
            executor: entry.executor.clone(),
        })
    }
}

/// Executes Task Runner tools against the locally resolved MCP runtime. Every
/// call reloads the durable Run and Task and validates their frozen snapshots
/// before any external I/O occurs.
pub struct FrozenCapabilityLocalToolRuntime {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    executors: Arc<dyn FrozenMcpExecutorProvider>,
}

impl FrozenCapabilityLocalToolRuntime {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        executors: Arc<dyn FrozenMcpExecutorProvider>,
    ) -> Self {
        Self {
            storage,
            scope,
            executors,
        }
    }

    async fn load_frozen_context(
        &self,
        invocation: &LocalToolInvocation,
    ) -> Result<FrozenToolContext, String> {
        let mut operation = LoadFrozenToolContext {
            scope: self.scope.clone(),
            run_id: invocation.run_id.clone(),
            result: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(|error| format!("failed to load frozen tool context: {error}"))?;
        let state = operation
            .result
            .ok_or_else(|| "frozen tool context query returned no state".to_string())?;
        frozen_tool_context_from_state(&self.scope, invocation, state)
    }
}

#[async_trait]
impl LocalToolRuntime for FrozenCapabilityLocalToolRuntime {
    async fn execute(
        &self,
        invocation: LocalToolInvocation,
        cancellation: CancellationToken,
    ) -> Result<LocalToolOutcome, String> {
        if cancellation.is_cancelled() {
            return Err("local MCP invocation was cancelled".to_string());
        }
        let context = self.load_frozen_context(&invocation).await?;
        let resolved = tokio::select! {
            _ = cancellation.cancelled() => {
                return Err("local MCP invocation was cancelled".to_string());
            }
            result = self.executors.resolve(
                &context.capability.plugin_release_snapshot,
                cancellation.clone(),
            ) => result?,
        };
        if resolved.plugin_release_snapshot != context.capability.plugin_release_snapshot {
            return Err(
                "resolved MCP plugins do not match the frozen plugin release snapshot".to_string(),
            );
        }
        validate_executor_tool(
            resolved.executor.as_ref(),
            &context.tool,
            invocation.tool_name.as_str(),
        )?;

        let cancelled = cancellation.clone();
        let abort_checker = Arc::new(move |_conversation_id: &str| cancelled.is_cancelled());
        let tool_call = json!({
            "id": invocation.tool_call_id,
            "function": {
                "name": invocation.tool_name,
                "arguments": invocation.arguments,
            }
        });
        let call_context = ToolCallContext::new(
            Some(invocation.run_id),
            Some(invocation.source_turn_id),
            None,
        )
        .with_abort_checker(abort_checker)
        .with_tool_result_max_chars(Some(TOOL_RESULT_MAX_CHARS));
        let results = tokio::select! {
            _ = cancellation.cancelled() => {
                return Err("local MCP invocation was cancelled".to_string());
            }
            results = resolved.executor.execute_tools_stream(
                std::slice::from_ref(&tool_call),
                call_context,
                None,
            ) => results,
        };
        if cancellation.is_cancelled() {
            return Err("local MCP invocation was cancelled".to_string());
        }
        let result = exactly_one_result(results)?;
        let bounded_result = bounded_tool_result(&result)?;
        Ok(if result.success && !result.is_error {
            LocalToolOutcome::succeeded(bounded_result)
        } else {
            LocalToolOutcome::failed(bounded_result)
        })
    }
}

struct FrozenToolContextState {
    run: AgentRunStateRecord,
    task: TaskRecord,
}

struct FrozenToolContext {
    capability: TaskRunnerCapabilitySnapshot,
    tool: TaskRunnerExecutionTool,
}

struct LoadFrozenToolContext {
    scope: RecordScope,
    run_id: String,
    result: Option<FrozenToolContextState>,
}

#[async_trait]
impl StorageTransaction for LoadFrozenToolContext {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.run_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let task = repositories
            .tasks()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: run.run.owner_entity_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        self.result = Some(FrozenToolContextState { run, task });
        Ok(())
    }
}

fn frozen_tool_context_from_state(
    scope: &RecordScope,
    invocation: &LocalToolInvocation,
    state: FrozenToolContextState,
) -> Result<FrozenToolContext, String> {
    let run = state.run.run;
    let project_id = invocation
        .project_id
        .as_deref()
        .ok_or_else(|| "Task Runner tool invocation has no frozen project_id".to_string())?;
    if state.run.metadata.id != invocation.run_id
        || state.run.metadata.scope != *scope
        || run.run_id != invocation.run_id
        || run.owner_user_id != scope.owner_user_id
        || run.profile_key != "task_runner"
        || run.owner_entity_type != "task"
        || run.project_id.as_deref() != Some(project_id)
        || run.capability_snapshot_ref != invocation.capability_snapshot_ref
    {
        return Err("tool invocation does not match the durable frozen Run".to_string());
    }
    let task = state.task;
    if task.metadata.id != run.owner_entity_id
        || task.metadata.scope != *scope
        || task.state.get("run_id").and_then(Value::as_str) != Some(run.run_id.as_str())
        || task.state.get("project_id").and_then(Value::as_str) != Some(project_id)
    {
        return Err("tool invocation does not match the durable frozen Task".to_string());
    }
    let capability_snapshot = snapshot_from_task(&task, "capability_snapshot")?;
    let project_snapshot = snapshot_from_task(&task, "project_snapshot")?;
    if capability_snapshot.snapshot_id != run.capability_snapshot_ref
        || capability_snapshot.snapshot_id != invocation.capability_snapshot_ref
    {
        return Err("Task capability snapshot does not match the tool invocation".to_string());
    }
    let capability: TaskRunnerCapabilitySnapshot =
        snapshot_payload(&capability_snapshot, "capability_snapshot")?;
    let project: TaskRunnerProjectSnapshot =
        snapshot_payload(&project_snapshot, "project_snapshot")?;
    capability.validate()?;
    project.validate()?;
    if capability.snapshot_ref != capability_snapshot.snapshot_id
        || project.project_id != project_id
        || project.snapshot_revision != project_snapshot.revision
    {
        return Err("frozen Task snapshot payload identity is invalid".to_string());
    }
    let tool = capability
        .execution_tools
        .iter()
        .find(|tool| tool.name == invocation.tool_name)
        .cloned()
        .ok_or_else(|| {
            format!(
                "tool {} is outside the frozen capability snapshot",
                invocation.tool_name
            )
        })?;
    if tool.effect != invocation.effect {
        return Err(format!(
            "tool {} effect does not match the frozen capability snapshot",
            invocation.tool_name
        ));
    }
    Ok(FrozenToolContext { capability, tool })
}

fn snapshot_from_task(task: &TaskRecord, field: &'static str) -> Result<FrozenSnapshot, String> {
    let snapshot: FrozenSnapshot = serde_json::from_value(
        task.state
            .get(field)
            .cloned()
            .ok_or_else(|| format!("durable Task has no {field}"))?,
    )
    .map_err(|error| format!("durable Task {field} is invalid: {error}"))?;
    snapshot
        .validate(field)
        .map_err(|error| format!("durable Task {field} failed integrity validation: {error}"))?;
    Ok(snapshot)
}

fn snapshot_payload<T: serde::de::DeserializeOwned>(
    snapshot: &FrozenSnapshot,
    field: &str,
) -> Result<T, String> {
    serde_json::from_value(snapshot.payload.clone())
        .map_err(|error| format!("{field} payload does not match its frozen contract: {error}"))
}

fn validate_executor_tool(
    executor: &McpExecutor,
    frozen: &TaskRunnerExecutionTool,
    requested_name: &str,
) -> Result<(), String> {
    let schema = executor
        .available_tools()
        .into_iter()
        .find(|schema| schema.get("name").and_then(Value::as_str) == Some(requested_name))
        .ok_or_else(|| format!("frozen MCP tool {requested_name} is not locally available"))?;
    if schema != frozen.schema {
        return Err(format!(
            "local MCP tool {requested_name} schema does not match the frozen schema"
        ));
    }
    let metadata = executor
        .tool_metadata()
        .get(requested_name)
        .ok_or_else(|| format!("local MCP tool {requested_name} has no execution metadata"))?;
    if REMOTE_EXECUTION_SERVERS.contains(&metadata.server_name.as_str()) {
        return Err(format!(
            "local Agent cannot execute through remote server {}",
            metadata.server_name
        ));
    }
    Ok(())
}

fn exactly_one_result(mut results: Vec<ToolResult>) -> Result<ToolResult, String> {
    if results.len() != 1 {
        return Err(format!(
            "local MCP invocation returned {} terminal results instead of one",
            results.len()
        ));
    }
    Ok(results.remove(0))
}

fn bounded_tool_result(result: &ToolResult) -> Result<Value, String> {
    let summary = sanitize_text(&result.content, TOOL_RESULT_MAX_CHARS);
    let structured_result = result.result.as_ref().map(sanitize_value);
    let mut value = json!({
        "summary": summary,
        "verification": result.success && !result.is_error,
        "fatal_error": result.fatal_error,
        "structured_result": structured_result,
    });
    if serialized_len(&value)? > MAX_BOUNDED_JSON_BYTES {
        value["structured_result"] = Value::Null;
    }
    if serialized_len(&value)? > MAX_BOUNDED_JSON_BYTES {
        value["summary"] = Value::String(sanitize_text(&result.content, 8_000));
    }
    let length = serialized_len(&value)?;
    if length > MAX_BOUNDED_JSON_BYTES {
        return Err(format!(
            "sanitized MCP result is {length} bytes; maximum is {MAX_BOUNDED_JSON_BYTES}"
        ));
    }
    Ok(value)
}

fn serialized_len(value: &Value) -> Result<usize, String> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|error| format!("failed to bound MCP result: {error}"))
}

fn sanitize_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(sanitize_text(text, TOOL_RESULT_MAX_CHARS)),
        Value::Array(values) => Value::Array(values.iter().map(sanitize_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), sanitize_value(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn sanitize_text(text: &str, maximum_chars: usize) -> String {
    let sanitized = text
        .split_inclusive(char::is_whitespace)
        .map(|part| {
            let token = part.trim_end_matches(char::is_whitespace);
            let suffix = &part[token.len()..];
            if looks_like_sensitive_local_path(token) {
                format!("[local-path-redacted]{suffix}")
            } else {
                part.to_string()
            }
        })
        .collect::<String>();
    if sanitized.chars().count() <= maximum_chars {
        sanitized
    } else {
        sanitized.chars().take(maximum_chars).collect::<String>() + "\n...[truncated]"
    }
}

fn looks_like_sensitive_local_path(value: &str) -> bool {
    value.starts_with("file://")
        || value.starts_with("/Users/")
        || value.starts_with("/Volumes/")
        || value.starts_with("/home/")
        || value.starts_with("/private/")
        || (value.len() > 3
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'\\' | b'/')
            && value.as_bytes()[0].is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::{looks_like_sensitive_local_path, sanitize_text};

    #[test]
    fn redacts_local_paths_without_redacting_web_routes() {
        let sanitized = sanitize_text(
            "saved /Users/alice/secret.txt and /pricing but not https://example.com/home",
            1_000,
        );
        assert!(sanitized.contains("[local-path-redacted]"));
        assert!(sanitized.contains("/pricing"));
        assert!(sanitized.contains("https://example.com/home"));
        assert!(looks_like_sensitive_local_path(
            "C:\\Users\\alice\\secret.txt"
        ));
    }

    #[test]
    fn truncates_utf8_on_character_boundaries() {
        assert_eq!(sanitize_text("设计画布", 2), "设计\n...[truncated]");
    }
}

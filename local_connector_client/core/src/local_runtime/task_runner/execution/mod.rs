// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod completion;

use std::sync::Arc;

use async_trait::async_trait;
use chatos_ai_runtime::{
    AiRuntimeOptions, RuntimeBeforeModelRequest, RuntimeIterationContext, RuntimeLifecycleHook,
    RuntimeRecordOptions, TaskMcpInitMode, TaskRunExecution, TaskRunSpec, TaskRuntime,
    TaskRuntimeConfig,
};
use chatos_plugin_management_sdk::{required_agent_prompt_vendor, SystemAgentKey};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::local_runtime::capabilities::merge_system_prompts;
use crate::local_runtime::chat::{
    prepare_local_chat_tools, LocalChatEventStream, LocalChatRecordWriter,
};
use crate::local_runtime::load_installed_agent_prompt;
use crate::local_runtime::model::build_local_model_config;
use crate::local_runtime::storage::{
    BeginLocalBackgroundTurnInput, BeginLocalTurnInput, BeginLocalTurnResult,
};
use crate::local_runtime::task_runner::LocalTaskRunRecord;
use crate::model_configs::resolve_local_model_runtime;
use crate::terminal::controller::{
    local_terminal_controller_context_for_task_run, LocalConnectorTerminalControllerStore,
};
use crate::LocalRuntime;

use self::completion::finish_task_run;
pub(crate) use self::completion::user_visible_task_run_failure_receipt;
pub(super) use self::completion::{
    persist_task_run_receipt, set_requirement_status, set_work_item_status,
};

// Keep local task execution aligned with the single shared Agent default.
// Do not introduce a local numeric override here: that previously caused the
// desktop Task Runner to stop earlier than ChatOS and the server Task Runner.
const LOCAL_TASK_RUN_MAX_ITERATIONS: usize = chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS;
const LOCAL_TASK_RUN_FINALIZATION_ITERATION: usize = LOCAL_TASK_RUN_MAX_ITERATIONS - 1;

struct LocalTaskFinalizationLifecycleHook;

#[async_trait]
impl RuntimeLifecycleHook for LocalTaskFinalizationLifecycleHook {
    async fn before_model_request(
        &self,
        context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        if context.iteration < LOCAL_TASK_RUN_FINALIZATION_ITERATION {
            return Ok(RuntimeBeforeModelRequest::unchanged());
        }
        Ok(RuntimeBeforeModelRequest::unchanged()
            .with_tools_enabled(false)
            .with_input_items(vec![json!({
                "role": "system",
                "content": "[Task Runner Finalization]\n工具执行预算即将结束。不要再调用任何工具。请根据已经完成的真实操作和验证结果，立即给用户输出简洁、准确的最终总结；如仍有未完成项，明确说明实际状态，不要声称已完成。"
            })]))
    }
}

pub(super) async fn execute_local_task_run(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    abort_token: CancellationToken,
) -> Result<(), String> {
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    database
        .get_session(run.session_id.as_str(), run.owner_user_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local Task Runner session was not found".to_string())?;
    let project = database
        .get_project(run.project_id.as_str(), run.owner_user_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local Task Runner project was not found".to_string())?;
    let settings = database
        .get_runtime_settings(run.owner_user_id.as_str(), run.session_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Local Task Runner settings were not found".to_string())?;
    let conversation_task = if run.task_kind == "conversation_task" {
        Some(
            database
                .get_local_task_board_task(
                    run.owner_user_id.as_str(),
                    run.session_id.as_str(),
                    run.task_id.as_str(),
                )
                .await
                .map_err(|error| error.to_string())?
                .filter(|task| task.task_kind == "task_runner")
                .ok_or_else(|| "Local Task Runner conversation task was not found".to_string())?,
        )
    } else {
        None
    };
    let work_item = if conversation_task.is_none() {
        Some(
            database
                .get_local_work_item(run.owner_user_id.as_str(), run.task_id.as_str())
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Local Task Runner work item was not found".to_string())?,
        )
    } else {
        None
    };
    let is_planning_task = conversation_task
        .as_ref()
        .map(|task| task.is_planning_task)
        .or_else(|| work_item.as_ref().map(|task| task.is_planning_task))
        .unwrap_or(false);
    let agent_key = if is_planning_task {
        SystemAgentKey::TaskRunnerPlanPhase
    } else {
        SystemAgentKey::TaskRunnerRunPhase
    };
    let mut task_settings = settings.clone();
    task_settings.selected_model_id = Some(run.model_config_id.clone());
    task_settings.plan_mode_enabled = is_planning_task;
    if let Some(task) = conversation_task.as_ref() {
        task_settings.mcp_enabled = true;
        task_settings.enabled_mcp_ids_json = serde_json::to_string(&conversation_task_mcp_ids(
            task.enabled_builtin_kinds.as_slice(),
            task.external_mcp_config_ids.as_slice(),
        )?)
        .map_err(|error| error.to_string())?;
        task_settings.selected_skill_ids_json =
            serde_json::to_string(&task.selected_skill_ids).map_err(|error| error.to_string())?;
    }
    begin_task_turn(database, run).await?;
    set_work_item_status(runtime, run, "in_progress").await?;
    let prepared = prepare_local_chat_tools(
        runtime,
        run.owner_user_id.as_str(),
        run.id.as_str(),
        &project,
        &task_settings,
        agent_key,
        conversation_task.is_none(),
    )
    .await?;
    let resolved_model = {
        let state = runtime.state.read().await;
        resolve_local_model_runtime(
            &state,
            run.owner_user_id.as_str(),
            run.model_config_id.as_str(),
        )
        .map_err(|error| error.to_string())?
    };
    let model_name = resolved_model.model.clone();
    let prompt_vendor = required_agent_prompt_vendor(
        resolved_model.prompt_vendor.as_deref(),
        resolved_model.provider.as_str(),
    )
    .map_err(|error| error.to_string())?;
    let installed_prompt = load_installed_agent_prompt(runtime, agent_key, prompt_vendor)
        .await
        .map_err(|error| error.to_string())?;
    let model = build_local_model_config(
        resolved_model,
        merge_system_prompts(
            Some(installed_prompt.content),
            prepared.capability_prompt.clone(),
        ),
        task_settings.selected_thinking_level.clone(),
        None,
        true,
        Some(prepared.project_root.display().to_string()),
    );
    let mut builder = TaskRuntime::builder()
        .with_record_writer_arc(Arc::new(LocalChatRecordWriter::new(
            database.clone(),
            run.owner_user_id.as_str(),
            run.session_id.as_str(),
            run.turn_id.as_str(),
        )))
        .with_max_iterations(LOCAL_TASK_RUN_MAX_ITERATIONS);
    if let Some(executor) = prepared.executor {
        builder = builder.with_tool_executor_arc(executor);
    }
    let spec = TaskRunSpec::new(
        run.task_id.clone(),
        run.id.clone(),
        model.clone(),
        run.prompt.clone(),
    )
    .with_model_config_id(run.model_config_id.clone())
    .with_tools(prepared.available_tools);
    let execution = TaskRunExecution::new(
        TaskRuntimeConfig::new().with_mcp_init_mode(TaskMcpInitMode::Disabled),
        spec,
    );
    let events = LocalChatEventStream::start(
        database.clone(),
        run.owner_user_id.as_str(),
        run.session_id.as_str(),
        run.turn_id.as_str(),
    );
    events.publish(
        "task.run.started",
        Some("status"),
        json!({ "run_id": run.id }),
    );
    let report = execution
        .run_report_with_runtime_options(
            &builder.build(),
            AiRuntimeOptions::new(Some(run.session_id.clone()), Some(run.turn_id.clone()))
                .with_caller_model(Some(model_name))
                .with_caller_model_runtime(Some(model.to_tool_caller_model_runtime()))
                .with_abort_token(Some(abort_token.clone()))
                .with_lifecycle_hook(Some(Arc::new(LocalTaskFinalizationLifecycleHook)))
                .with_callbacks(events.callbacks())
                .with_record_options(RuntimeRecordOptions::persist_all()),
        )
        .await;
    let _ = events.finish().await;
    let cleanup_context = local_terminal_controller_context_for_task_run(
        prepared.project_root.as_path(),
        run.owner_user_id.as_str(),
        run.id.as_str(),
        30_000,
    );
    if let Err(error) = LocalConnectorTerminalControllerStore
        .kill_sessions_for_context(cleanup_context)
        .await
    {
        crate::tracing_stdout(
            format!("local task run {} terminal cleanup failed: {error}", run.id).as_str(),
        );
    }
    finish_task_run(runtime, run, report, abort_token.is_cancelled()).await
}

#[cfg(test)]
mod execution_policy_tests {
    use chatos_ai_runtime::{RuntimeIterationContext, RuntimeLifecycleHook};
    use serde_json::Value;

    use super::{
        task_run_idempotency_key, LocalTaskFinalizationLifecycleHook,
        LOCAL_TASK_RUN_FINALIZATION_ITERATION, LOCAL_TASK_RUN_MAX_ITERATIONS,
    };

    #[tokio::test]
    async fn implementation_tasks_reserve_a_tool_free_finalization_round() {
        assert_eq!(LOCAL_TASK_RUN_MAX_ITERATIONS, 600);
        assert!(LOCAL_TASK_RUN_FINALIZATION_ITERATION < LOCAL_TASK_RUN_MAX_ITERATIONS);

        let normal = LocalTaskFinalizationLifecycleHook
            .before_model_request(iteration_context(1))
            .await
            .expect("normal task iteration");
        assert!(normal.tools_enabled);
        assert!(normal.input_items.is_empty());

        let finalization = LocalTaskFinalizationLifecycleHook
            .before_model_request(iteration_context(LOCAL_TASK_RUN_FINALIZATION_ITERATION))
            .await
            .expect("task finalization iteration");
        assert!(!finalization.tools_enabled);
        assert_eq!(finalization.input_items.len(), 1);
        assert!(finalization.input_items[0]
            .to_string()
            .contains("不要再调用任何工具"));
    }

    #[test]
    fn retry_attempts_use_distinct_turn_idempotency_keys() {
        assert_ne!(
            task_run_idempotency_key("run-1", 1),
            task_run_idempotency_key("run-1", 2),
        );
    }

    fn iteration_context(iteration: usize) -> RuntimeIterationContext {
        RuntimeIterationContext {
            conversation_id: Some("session-1".to_string()),
            conversation_turn_id: Some("turn-1".to_string()),
            iteration,
            reason: "tool_results".to_string(),
            input: Value::Array(Vec::new()),
        }
    }
}

async fn begin_task_turn(
    database: &crate::local_runtime::LocalDatabase,
    run: &LocalTaskRunRecord,
) -> Result<(), String> {
    if run.task_kind == "conversation_task" {
        let task = database
            .get_local_task_board_task(
                run.owner_user_id.as_str(),
                run.session_id.as_str(),
                run.task_id.as_str(),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Local Task Runner conversation task was not found".to_string())?;
        return match database
            .begin_background_turn(BeginLocalBackgroundTurnInput {
                session_id: run.session_id.clone(),
                owner_user_id: run.owner_user_id.clone(),
                source_turn_id: task.source_turn_id,
                turn_id: run.turn_id.clone(),
                idempotency_key: task_run_idempotency_key(run.id.as_str(), run.attempt),
            })
            .await
            .map_err(|error| error.to_string())?
        {
            BeginLocalTurnResult::Started(_) => Ok(()),
            BeginLocalTurnResult::Existing(snapshot) if snapshot.turn.status == "completed" => {
                Ok(())
            }
            BeginLocalTurnResult::Existing(_) => {
                Err("Local Task Runner background turn already exists".to_string())
            }
        };
    }
    match database
        .begin_turn(BeginLocalTurnInput {
            session_id: run.session_id.clone(),
            owner_user_id: run.owner_user_id.clone(),
            turn_id: run.turn_id.clone(),
            idempotency_key: task_run_idempotency_key(run.id.as_str(), run.attempt),
            content: run.prompt.clone(),
            metadata_json: Some(
                json!({
                    "runtime_origin": "local_device", "message_mode": "task_run",
                    "task_id": run.task_id, "run_id": run.id,
                })
                .to_string(),
            ),
        })
        .await
        .map_err(|error| error.to_string())?
    {
        BeginLocalTurnResult::Started(_) => Ok(()),
        BeginLocalTurnResult::Existing(snapshot) if snapshot.turn.status == "completed" => Ok(()),
        BeginLocalTurnResult::Existing(_) => {
            Err("Local Task Runner turn already exists".to_string())
        }
    }
}

fn conversation_task_mcp_ids(
    builtin_kinds: &[String],
    external_mcp_config_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for value in builtin_kinds {
        let kind = chatos_mcp_runtime::builtin_kind_by_any(value.as_str())
            .ok_or_else(|| format!("Unknown local Task Runner builtin capability: {value}"))?;
        let descriptor = chatos_mcp::system_mcp_descriptor_by_embedded_kind(kind)
            .ok_or_else(|| format!("Missing system MCP descriptor for {value}"))?;
        ids.push(descriptor.resource_id.to_string());
    }
    for id in external_mcp_config_ids {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    Ok(ids)
}

fn task_run_idempotency_key(run_id: &str, attempt: i64) -> String {
    format!("{run_id}:attempt:{}", attempt.max(1))
}

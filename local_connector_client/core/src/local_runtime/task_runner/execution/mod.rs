// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod completion;

use std::sync::Arc;

use chatos_ai_runtime::{
    AiRuntimeOptions, RuntimeRecordOptions, TaskMcpInitMode, TaskRunExecution, TaskRunSpec,
    TaskRuntime, TaskRuntimeConfig,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::local_runtime::chat::{
    prepare_local_chat_tools, LocalChatEventStream, LocalChatRecordWriter,
};
use crate::local_runtime::model::build_local_model_config;
use crate::local_runtime::storage::{BeginLocalTurnInput, BeginLocalTurnResult};
use crate::local_runtime::task_runner::LocalTaskRunRecord;
use crate::model_configs::resolve_local_model_runtime;
use crate::LocalRuntime;

use self::completion::{finish_task_run, set_work_item_status};

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
    begin_task_turn(database, run).await?;
    set_work_item_status(runtime, run, "in_progress").await?;
    let prepared = prepare_local_chat_tools(
        runtime,
        run.owner_user_id.as_str(),
        run.id.as_str(),
        &project,
        &settings,
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
    let model = build_local_model_config(
        resolved_model,
        prepared.capability_prompt.clone(),
        settings.selected_thinking_level.clone(),
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
        .with_max_iterations(20);
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
                .with_callbacks(events.callbacks())
                .with_record_options(RuntimeRecordOptions::persist_all()),
        )
        .await;
    let _ = events.finish().await;
    finish_task_run(runtime, run, report, abort_token.is_cancelled()).await
}

async fn begin_task_turn(
    database: &crate::local_runtime::LocalDatabase,
    run: &LocalTaskRunRecord,
) -> Result<(), String> {
    match database
        .begin_turn(BeginLocalTurnInput {
            session_id: run.session_id.clone(),
            owner_user_id: run.owner_user_id.clone(),
            turn_id: run.turn_id.clone(),
            idempotency_key: run.id.clone(),
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

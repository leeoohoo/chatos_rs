// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{
    BatchTaskRunRequest, RunListFilters, StartTaskRunRequest, TaskMemoryContextOptions,
    TaskMemoryRecordsOptions,
};

use super::{
    decode_args, text_result, BatchTaskRunArgs, GetTaskMemoryContextArgs, ListRunsArgs,
    ListTaskMemoryRecordsArgs, McpRequestContext, RunIdArgs, StartTaskRunArgs, TaskIdArgs,
    TaskRunnerMcpService,
};

impl TaskRunnerMcpService {
    pub(super) async fn call_run_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        match name {
            "list_runs" => {
                let args: ListRunsArgs = decode_args(args)?;
                if let Some(task_id) = args.task_id.as_deref() {
                    self.require_task_for_user_in_context(task_id, current_user, request_context)
                        .await?;
                }
                let runs = self
                    .run_service
                    .list_runs_filtered(RunListFilters {
                        task_id: args.task_id,
                        status: args.status,
                        model_config_id: args.model_config_id,
                        keyword: None,
                        limit: args.limit,
                        offset: None,
                    })
                    .await?;
                let runs = self
                    .filter_runs_for_user_in_context(runs, current_user, request_context)
                    .await?;
                Ok(text_result(json!(runs)))
            }
            "get_run" => {
                let args: RunIdArgs = decode_args(args)?;
                let run = self
                    .require_run_for_user_in_context(
                        args.run_id.as_str(),
                        current_user,
                        request_context,
                    )
                    .await?;
                Ok(text_result(json!(run)))
            }
            "start_task_run" => {
                let args: StartTaskRunArgs = decode_args(args)?;
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let run = self
                    .run_service
                    .start_run_for_user(
                        args.task_id.as_str(),
                        StartTaskRunRequest {
                            model_config_id: args.model_config_id,
                            prompt_override: args.prompt_override,
                        },
                        current_user,
                    )
                    .await?;
                Ok(text_result(json!(run)))
            }
            "batch_start_task_runs" => {
                let args: BatchTaskRunArgs = decode_args(args)?;
                self.require_tasks_for_user_in_context(
                    args.task_ids.as_slice(),
                    current_user,
                    request_context,
                )
                .await?;
                let result = self
                    .run_service
                    .batch_start_runs_for_user(
                        BatchTaskRunRequest {
                            task_ids: args.task_ids,
                            model_config_id: args.model_config_id,
                            prompt_override: args.prompt_override,
                        },
                        current_user,
                    )
                    .await?;
                Ok(text_result(json!(result)))
            }
            "get_task_memory_context" => {
                let args: GetTaskMemoryContextArgs = decode_args(args)?;
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let response = self
                    .task_service
                    .get_task_memory_context(
                        args.task_id.as_str(),
                        TaskMemoryContextOptions {
                            include_recent_records: args.include_recent_records,
                            include_thread_summary: args.include_thread_summary,
                            include_subject_memory: args.include_subject_memory,
                            recent_record_limit: args.recent_record_limit,
                            summary_limit: args.summary_limit,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("task not found: {}", args.task_id))?;
                Ok(text_result(json!(response)))
            }
            "list_task_memory_records" => {
                let args: ListTaskMemoryRecordsArgs = decode_args(args)?;
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let response = self
                    .task_service
                    .get_task_memory_records(
                        args.task_id.as_str(),
                        TaskMemoryRecordsOptions {
                            role: args.role,
                            record_type: args.record_type,
                            summary_status: args.summary_status,
                            limit: args.limit,
                            offset: args.offset,
                            order: args.order,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("task not found: {}", args.task_id))?;
                Ok(text_result(json!(response)))
            }
            "summarize_task_memory" => {
                let args: TaskIdArgs = decode_args(args)?;
                self.require_task_for_user_in_context(
                    args.task_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let response = self
                    .task_service
                    .summarize_task_memory(args.task_id.as_str())
                    .await?
                    .ok_or_else(|| format!("task not found: {}", args.task_id))?;
                Ok(text_result(json!(response)))
            }
            "cancel_run" => {
                let args: RunIdArgs = decode_args(args)?;
                self.require_run_for_user_in_context(
                    args.run_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let run = self
                    .run_service
                    .cancel_run(args.run_id.as_str())
                    .await?
                    .ok_or_else(|| format!("run not found: {}", args.run_id))?;
                Ok(text_result(json!(run)))
            }
            "retry_run" => {
                let args: RunIdArgs = decode_args(args)?;
                self.require_run_for_user_in_context(
                    args.run_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let run = self
                    .run_service
                    .retry_run_for_user(args.run_id.as_str(), current_user)
                    .await?
                    .ok_or_else(|| format!("run not found: {}", args.run_id))?;
                Ok(text_result(json!(run)))
            }
            "list_run_events" => {
                let args: RunIdArgs = decode_args(args)?;
                self.require_run_for_user_in_context(
                    args.run_id.as_str(),
                    current_user,
                    request_context,
                )
                .await?;
                let events = self
                    .run_service
                    .list_run_events(args.run_id.as_str())
                    .await?;
                Ok(text_result(json!(events)))
            }
            other => Err(format!("unsupported run tool: {other}")),
        }
    }
}

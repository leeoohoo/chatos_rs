#![cfg_attr(not(test), allow(dead_code))]

use chatos_ai_runtime::{
    tool_call::{
        build_function_call_item, build_function_call_output_item, extract_tool_call_id,
        extract_tool_call_name, parse_tool_calls_value, tool_call_arguments_text,
    },
    AiRuntime, AiRuntimeOptions, AiSingleStepOutcome, AiSingleStepRequest, ModelRequest,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

use crate::authentication::{AuthenticationManager, GatewayModelConfig};

use super::{
    execution::StoryPlanningExecutionLease,
    media::StoryStudioManager,
    memory::{
        assemble_memory_input, bind_memory, memory_records, synced_digest, StoryMemoryAdapter,
    },
    planning::{
        project_fingerprint, StoryAgentMessage, StoryAgentMessageRole, StoryAgentReceipt,
        StoryAgentRunPolicy, StoryAgentToolCall, StoryAgentUsage, StoryPlanningEvent,
        StoryPlanningRun, StoryPlanningRunSnapshot, StoryPlanningStage, StoryPlanningStatus,
    },
    projection::StoryProjectSnapshot,
    tools,
};

const STORY_PLANNING_EVENT: &str = "story-planning-run";
const MAXIMUM_TOOL_CALLS_PER_RESPONSE: usize = 32;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryPlanningOutcome {
    pub run: StoryPlanningRunSnapshot,
    pub project: Option<StoryProjectSnapshot>,
    pub stop_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoryPlanningEventEnvelope {
    presentation_id: String,
    run: StoryPlanningRunSnapshot,
}

pub(super) struct StoryPlanningRuntime<'a> {
    pub authentication: &'a AuthenticationManager,
    pub manager: &'a StoryStudioManager,
    pub app: &'a AppHandle,
    pub owner_user_id: &'a str,
    pub authentication_generation: u64,
    pub presentation_id: &'a str,
}

impl StoryPlanningRuntime<'_> {
    pub(super) async fn start(
        &self,
        project_id: &str,
        stage: StoryPlanningStage,
        target_ids: Vec<String>,
        user_ideas: String,
    ) -> Result<StoryPlanningOutcome, String> {
        self.require_current().await?;
        let store = self.manager.store();
        let owner = self.owner_user_id.to_owned();
        let load_project_id = project_id.to_owned();
        let project = tauri::async_runtime::spawn_blocking(move || {
            store.load_native_project(&owner, &load_project_id)
        })
        .await
        .map_err(|_| "剧情项目读取任务异常退出。".to_owned())??;
        self.require_current().await?;
        let now = current_unix_milliseconds();
        let system_prompt = tools::system_prompt(&project);
        let source_length = project.source.chars().count();
        let target_count = target_ids.len();
        let goal_ideas = user_ideas.clone();
        let run = StoryPlanningRun::new(
            &project,
            self.owner_user_id,
            stage,
            target_ids,
            &user_ideas,
            StoryAgentRunPolicy::default(),
            system_prompt,
            move |ideas| {
                tools::goal_prompt(
                    stage,
                    source_length,
                    target_count,
                    if ideas.is_empty() { &goal_ideas } else { ideas },
                )
            },
            now,
        )?;
        self.run(run, false).await
    }

    pub(super) async fn resume(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> Result<StoryPlanningOutcome, String> {
        self.require_current().await?;
        let store = self.manager.store();
        let owner = self.owner_user_id.to_owned();
        let project_id_owned = project_id.to_owned();
        let run_id_owned = run_id.to_owned();
        let (project, run) = tauri::async_runtime::spawn_blocking(move || {
            let project = store.load_native_project(&owner, &project_id_owned)?;
            let run = store.load_planning_run(&owner, &project_id_owned, &run_id_owned)?;
            Ok::<_, String>((project, run))
        })
        .await
        .map_err(|_| "剧情规划恢复任务异常退出。".to_owned())??;
        self.require_current().await?;
        if run.applied || run.abandoned_at_unix_ms.is_some() {
            return Err("剧情规划运行已经结束，不能恢复。".into());
        }
        let current = project_fingerprint(&project)?;
        let draft = project_fingerprint(&run.draft)?;
        if current != run.base_project_fingerprint
            && !(run.checkpoint.status == StoryPlanningStatus::Completed && current == draft)
        {
            return Err("项目已被修改，不能恢复旧规划；请保留草稿并启动新的规划。".into());
        }
        self.run(run, true).await
    }

    async fn run(
        &self,
        mut run: StoryPlanningRun,
        resume: bool,
    ) -> Result<StoryPlanningOutcome, String> {
        let lease = self.manager.planning_executions.begin(
            self.owner_user_id,
            self.authentication_generation,
            self.presentation_id,
            &run.project_id,
            run.id(),
        )?;
        let result = self.run_with_lease(&mut run, resume, &lease).await;
        self.manager.planning_executions.finish(&lease);
        result
    }

    async fn run_with_lease(
        &self,
        run: &mut StoryPlanningRun,
        resume: bool,
        lease: &StoryPlanningExecutionLease,
    ) -> Result<StoryPlanningOutcome, String> {
        if resume {
            prepare_for_resume(run)?;
        }
        let memory = StoryMemoryAdapter::for_owner(self.authentication, self.owner_user_id).await?;
        bind_memory(run, self.owner_user_id, current_unix_milliseconds())?;
        self.persist(run, lease, "started", "Agent 运行开始 / 恢复")
            .await?;
        if run.checkpoint.status == StoryPlanningStatus::Completed {
            return self.apply_completed(run, lease).await;
        }
        run.checkpoint.status = StoryPlanningStatus::Running;
        run.checkpoint.stop_reason = None;
        self.persist(run, lease, "running", "剧情规划已进入运行状态")
            .await?;

        let model = match self.resolve_model(run).await {
            Ok(model) => model,
            Err(error) => {
                let message = error.clone();
                let _ = self.fail_model(run, lease, &message).await;
                return Err(error);
            }
        };
        let runtime = AiRuntime::builder()
            .with_max_iterations(run.policy.maximum_model_calls as usize)
            .with_request_read_timeout(Duration::from_secs(
                run.policy.request_timeout_seconds as u64,
            ))
            .build_runtime();
        let mut active_model_input: Option<Vec<Value>> = None;
        let mut active_checkpoint_message_count = run.checkpoint.messages.len();
        let mut last_accounting = Instant::now();
        loop {
            if let Err(message) = self.require_active(lease).await {
                run.checkpoint.status = StoryPlanningStatus::Paused;
                run.checkpoint.stop_reason = Some(message.clone());
                let _ = self
                    .persist(run, lease, "paused", "账号或页面状态已变化，运行已暂停")
                    .await;
                return Err(message);
            }
            if let Err(error) = self.synchronize_memory(run, lease, &memory).await {
                let _ = self.fail_memory(run, lease, &error).await;
                return Err(error);
            }
            if let Some(input) = active_model_input.as_mut() {
                if active_checkpoint_message_count > run.checkpoint.messages.len() {
                    return Err("剧情规划历史记录与当前运行不匹配。".into());
                }
                if active_checkpoint_message_count < run.checkpoint.messages.len() {
                    input.extend(checkpoint_input(
                        &run.checkpoint.messages[active_checkpoint_message_count..],
                    )?);
                    active_checkpoint_message_count = run.checkpoint.messages.len();
                }
            }
            let now = Instant::now();
            run.checkpoint.elapsed_seconds += now.duration_since(last_accounting).as_secs_f64();
            last_accounting = now;
            if run.checkpoint.elapsed_seconds >= f64::from(run.policy.run_timeout_seconds) {
                return self
                    .stop(
                        run,
                        lease,
                        StoryPlanningStatus::Failed,
                        "剧情规划已达到运行时限。",
                        "timeout",
                    )
                    .await;
            }
            if run.checkpoint.pending_calls.is_empty() && tools::validate_completion(run).is_ok() {
                run.checkpoint.completion_result =
                    Some("本阶段规划完成，已通过客户端校验。".into());
                run.checkpoint.result = run.checkpoint.completion_result.clone();
                run.checkpoint.status = StoryPlanningStatus::Completed;
                self.persist(run, lease, "completed", "业务完成条件已确认")
                    .await?;
                return self.apply_completed(run, lease).await;
            }
            if !run.checkpoint.pending_calls.is_empty() {
                self.execute_pending_call(run, lease).await?;
                continue;
            }
            if run.checkpoint.model_calls >= run.policy.maximum_model_calls {
                return self
                    .stop(
                        run,
                        lease,
                        StoryPlanningStatus::LimitReached,
                        "已达到设置中的模型调用上限，未完成的操作需要人工确认。",
                        "limit_reached",
                    )
                    .await;
            }
            if run.checkpoint.no_progress_rounds >= u32::from(run.policy.maximum_no_progress_rounds)
            {
                return self
                    .stop(
                        run,
                        lease,
                        StoryPlanningStatus::Paused,
                        "连续无进展，已暂停。请检查工具错误或调整设置后继续。",
                        "no_progress",
                    )
                    .await;
            }
            if active_model_input.is_none() {
                let scope = run
                    .checkpoint
                    .memory
                    .as_ref()
                    .ok_or_else(|| "剧情规划历史记录无效。".to_owned())?
                    .scope
                    .clone();
                let response = match tokio::select! {
                    _ = lease.cancellation.cancelled() => Err("剧情规划已取消。".into()),
                    response = memory.compose(self.owner_user_id, &scope) => response,
                } {
                    Ok(response) => response,
                    Err(error) => {
                        let _ = self.fail_memory(run, lease, &error).await;
                        return Err(error);
                    }
                };
                self.require_active(lease).await?;
                let assembled = match assemble_memory_input(run, &response) {
                    Ok(input) => input,
                    Err(error) => {
                        let _ = self.fail_memory(run, lease, &error).await;
                        return Err(error);
                    }
                };
                active_model_input = Some(assembled);
                active_checkpoint_message_count = run.checkpoint.messages.len();
                self.persist(run, lease, "memory_composed", "已装配当前运行的历史上下文")
                    .await?;
            }
            if let Err(error) = self
                .model_step(
                    run,
                    lease,
                    &model,
                    &runtime,
                    active_model_input
                        .as_ref()
                        .ok_or_else(|| "剧情规划历史上下文无效。".to_owned())?,
                )
                .await
            {
                if lease.cancellation.is_cancelled()
                    || !self.manager.planning_executions.accepts(lease)
                {
                    return Err(error);
                }
                if run.checkpoint.status == StoryPlanningStatus::Running {
                    let message = error.clone();
                    let _ = self.fail_model(run, lease, &message).await;
                }
                return Err(error);
            }
        }
    }

    async fn model_step(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
        model: &ResolvedStoryModel,
        runtime: &AiRuntime,
        active_model_input: &[Value],
    ) -> Result<(), String> {
        let mut attempt = 1usize;
        loop {
            if run.checkpoint.model_calls >= run.policy.maximum_model_calls {
                return Ok(());
            }
            run.checkpoint.model_calls = run.checkpoint.model_calls.saturating_add(1);
            self.persist(
                run,
                lease,
                "model_request",
                &format!(
                    "模型调用 {} / {}",
                    run.checkpoint.model_calls, run.policy.maximum_model_calls
                ),
            )
            .await?;
            let input = Value::Array(active_model_input.to_vec());
            let request_body_limit = usize::try_from(
                run.policy
                    .context
                    .window_tokens
                    .saturating_sub(run.policy.context.output_reserve_tokens),
            )
            .unwrap_or(usize::MAX / 4)
            .saturating_mul(4)
            .saturating_add(2 * 1_024 * 1_024);
            if serde_json::to_vec(&input)
                .map_err(|_| "剧情规划模型历史无效。".to_owned())?
                .len()
                > request_body_limit
            {
                return Err(
                    "剧情规划上下文超过安全窗口，需要从已保存的业务检查点启动新运行。".into(),
                );
            }
            let request = ModelRequest::openai_compatible(
                model.base_url.clone(),
                model.api_key.clone(),
                model.model.clone(),
                model.provider.clone(),
                input,
            )
            .with_responses_support(model.supports_responses)
            .with_instructions(
                run.checkpoint
                    .messages
                    .iter()
                    .find(|message| message.role == StoryAgentMessageRole::System)
                    .map(|message| message.content.clone()),
            )
            .with_tools(
                tools::definitions(run.stage)
                    .iter()
                    .map(|value| value.provider_value())
                    .collect(),
            )
            .with_temperature(model.temperature)
            .with_max_output_tokens(model.max_output_tokens)
            .with_thinking_level(model.thinking_level.clone())
            .with_request_body_limit_bytes(Some(request_body_limit))
            .with_max_transient_retries(Some(run.policy.maximum_request_retries as usize));
            let options = AiRuntimeOptions::for_conversation(run.checkpoint.scope.clone())
                .with_conversation_turn_id(run.id().to_owned())
                .with_abort_token(Some(lease.cancellation.clone()));
            let outcome = runtime
                .execute_once(AiSingleStepRequest {
                    model_request: request,
                    runtime_options: options,
                    iteration: run.checkpoint.model_calls as usize,
                    reason: if attempt == 1 {
                        "story_planning"
                    } else {
                        "story_planning_retry"
                    }
                    .into(),
                    model_attempt: attempt,
                    force_identity_encoding: false,
                })
                .await?;
            self.require_active(lease).await?;
            match outcome {
                AiSingleStepOutcome::ToolCommand {
                    response,
                    tool_calls,
                } => {
                    self.record_model_response(run, response, Some(tool_calls), lease)
                        .await?;
                    return Ok(());
                }
                AiSingleStepOutcome::Final(response) => {
                    self.record_model_response(run, response, None, lease)
                        .await?;
                    run.checkpoint.messages.push(StoryAgentMessage {
                        role: StoryAgentMessageRole::User,
                        content: "请根据已保存的业务状态继续调用工具；只有调用结束工具并通过业务校验才能结束。".into(),
                        tool_calls: Vec::new(),
                        tool_call_id: None,
                        response_output_json: None,
                        usage: None,
                    });
                    self.persist(run, lease, "model_response", "模型未调用工具，已要求继续")
                        .await?;
                    return Ok(());
                }
                AiSingleStepOutcome::Continue { response, .. } => {
                    self.record_model_response(run, response, None, lease)
                        .await?;
                    run.checkpoint.messages.push(StoryAgentMessage {
                        role: StoryAgentMessageRole::User,
                        content: "请继续调用剧情规划工具推进尚未完成的业务状态。".into(),
                        tool_calls: Vec::new(),
                        tool_call_id: None,
                        response_output_json: None,
                        usage: None,
                    });
                    self.persist(run, lease, "model_response", "模型返回空结果，已要求继续")
                        .await?;
                    return Ok(());
                }
                AiSingleStepOutcome::Retry {
                    error,
                    next_model_attempt,
                    backoff_ms,
                    ..
                } => {
                    if attempt > run.policy.maximum_request_retries as usize {
                        return self.fail_model(run, lease, &error).await;
                    }
                    self.persist(run, lease, "model_retry", "模型请求暂时失败，正在重试")
                        .await?;
                    tokio::select! {
                        _ = lease.cancellation.cancelled() => return Err("剧情规划已取消。".into()),
                        _ = tokio::time::sleep(Duration::from_millis(backoff_ms)) => {}
                    }
                    attempt = next_model_attempt;
                }
                AiSingleStepOutcome::Failed { error } => {
                    return self.fail_model(run, lease, &error).await;
                }
                AiSingleStepOutcome::Cancelled => {
                    return self
                        .stop::<()>(
                            run,
                            lease,
                            StoryPlanningStatus::Paused,
                            "剧情规划已取消，检查点已保存。",
                            "paused",
                        )
                        .await;
                }
            }
        }
    }

    async fn record_model_response(
        &self,
        run: &mut StoryPlanningRun,
        response: chatos_ai_runtime::AiRuntimeResult,
        tool_calls: Option<Value>,
        lease: &StoryPlanningExecutionLease,
    ) -> Result<(), String> {
        let calls = tool_calls
            .as_ref()
            .map(parse_story_tool_calls)
            .transpose()?
            .unwrap_or_default();
        if calls.len() > MAXIMUM_TOOL_CALLS_PER_RESPONSE
            || calls
                .iter()
                .map(|value| &value.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != calls.len()
            || calls.iter().any(|value| {
                value.id.trim().is_empty()
                    || value.name.trim().is_empty()
                    || run.checkpoint.receipts.contains_key(&value.id)
            })
        {
            return Err("模型没有返回有效且完整的工具调用。".into());
        }
        let usage = response.usage.as_ref().map(usage_from_value);
        if let Some(usage) = &usage {
            let accumulated = run.checkpoint.usage.get_or_insert(StoryAgentUsage {
                input_tokens: 0,
                cached_tokens: 0,
                output_tokens: 0,
                requests: 0,
            });
            accumulated.input_tokens = accumulated.input_tokens.saturating_add(usage.input_tokens);
            accumulated.cached_tokens = accumulated
                .cached_tokens
                .saturating_add(usage.cached_tokens);
            accumulated.output_tokens = accumulated
                .output_tokens
                .saturating_add(usage.output_tokens);
            accumulated.requests = accumulated.requests.saturating_add(usage.requests);
        }
        run.checkpoint.no_progress_rounds = run.checkpoint.no_progress_rounds.saturating_add(1);
        run.checkpoint.messages.push(StoryAgentMessage {
            role: StoryAgentMessageRole::Assistant,
            content: response.content,
            tool_calls: calls.clone(),
            tool_call_id: None,
            response_output_json: Some(
                serde_json::to_vec(&response.response_output_items)
                    .map_err(|_| "模型响应无法保存。".to_owned())?,
            ),
            usage,
        });
        if calls.len() > 1 && calls.iter().any(|value| value.name == "story_finish") {
            for call in calls {
                let rejected = super::planning::StoryAgentToolOutcome {
                    content: "结束工具必须单独调用；本批次没有执行任何工具。".into(),
                    made_progress: false,
                    is_error: true,
                };
                run.checkpoint
                    .receipts
                    .insert(call.id.clone(), rejected.clone());
                run.checkpoint.messages.push(StoryAgentMessage {
                    role: StoryAgentMessageRole::Tool,
                    content: rejected.content,
                    tool_calls: Vec::new(),
                    tool_call_id: Some(call.id),
                    response_output_json: None,
                    usage: None,
                });
            }
        } else {
            run.checkpoint.pending_calls = calls;
        }
        self.persist(
            run,
            lease,
            "model_response",
            &format!("收到 {} 个工具调用", run.checkpoint.pending_calls.len()),
        )
        .await
    }

    async fn execute_pending_call(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
    ) -> Result<(), String> {
        let call = run
            .checkpoint
            .pending_calls
            .first()
            .cloned()
            .ok_or_else(|| "剧情规划工具队列无效。".to_owned())?;
        if let Some(saved) = run.checkpoint.receipts.get(&call.id).cloned() {
            run.checkpoint.messages.push(StoryAgentMessage {
                role: StoryAgentMessageRole::Tool,
                content: saved.content,
                tool_calls: Vec::new(),
                tool_call_id: Some(call.id),
                response_output_json: None,
                usage: None,
            });
            run.checkpoint.pending_calls.remove(0);
            run.checkpoint.in_flight_call_id = None;
            self.persist(run, lease, "tool_recovered", "已恢复持久化工具结果")
                .await?;
            return Ok(());
        }
        run.checkpoint.in_flight_call_id = Some(call.id.clone());
        self.persist(run, lease, "tool_started", &call.name).await?;
        self.require_active(lease).await?;

        let mut next = run.clone();
        let outcome = tools::execute(&call, &mut next);
        let fingerprint = call_fingerprint(&call);
        let repeated = next
            .checkpoint
            .observations
            .get(&fingerprint)
            .is_some_and(|value| value == &outcome.content);
        let mut model_outcome = outcome.clone();
        if repeated {
            model_outcome.made_progress = false;
        }
        next.checkpoint
            .observations
            .insert(fingerprint.clone(), outcome.content.clone());
        next.checkpoint
            .call_fingerprints
            .insert(call.id.clone(), fingerprint);
        next.checkpoint
            .receipts
            .insert(call.id.clone(), outcome.clone());
        next.tool_receipts.insert(
            call.id.clone(),
            StoryAgentReceipt {
                name: call.name.clone(),
                arguments: call.arguments.clone(),
                outcome: outcome.clone(),
            },
        );
        next.checkpoint.in_flight_call_id = None;
        next.checkpoint.pending_calls.remove(0);
        next.checkpoint.messages.push(StoryAgentMessage {
            role: StoryAgentMessageRole::Tool,
            content: model_outcome.content.clone(),
            tool_calls: Vec::new(),
            tool_call_id: Some(call.id),
            response_output_json: None,
            usage: None,
        });
        if model_outcome.made_progress && !model_outcome.is_error {
            next.checkpoint.no_progress_rounds = 0;
        }
        if call.name == "story_finish" && !outcome.is_error {
            next.checkpoint.completion_result = Some(outcome.content.clone());
        }
        self.require_active(lease).await?;
        *run = next;
        self.persist(
            run,
            lease,
            if model_outcome.is_error {
                "tool_rejected"
            } else {
                "tool_completed"
            },
            &format!("{}：{}", call.name, clipped(&model_outcome.content, 400)),
        )
        .await
    }

    async fn synchronize_memory(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
        adapter: &StoryMemoryAdapter,
    ) -> Result<(), String> {
        loop {
            let memory = run
                .checkpoint
                .memory
                .as_ref()
                .ok_or_else(|| "剧情规划历史记录无效。".to_owned())?
                .clone();
            if !memory.thread_created {
                self.persist(run, lease, "memory_connecting", "正在连接当前运行的历史")
                    .await?;
                tokio::select! {
                    _ = lease.cancellation.cancelled() => return Err("剧情规划已取消。".into()),
                    result = adapter.ensure_thread(self.owner_user_id, &memory.scope) => result?,
                }
                self.require_active(lease).await?;
                run.checkpoint
                    .memory
                    .as_mut()
                    .ok_or_else(|| "剧情规划历史记录无效。".to_owned())?
                    .thread_created = true;
                self.persist(run, lease, "memory_connected", "运行历史已连接")
                    .await?;
                continue;
            }
            if memory.synced_message_count >= run.checkpoint.messages.len() {
                return Ok(());
            }
            let start = memory.synced_message_count;
            let reconciling = memory.sync_in_flight_end.is_some();
            let end = memory
                .sync_in_flight_end
                .unwrap_or_else(|| (start + 32).min(run.checkpoint.messages.len()));
            if !reconciling {
                run.checkpoint
                    .memory
                    .as_mut()
                    .ok_or_else(|| "剧情规划历史记录无效。".to_owned())?
                    .sync_in_flight_end = Some(end);
                self.persist(
                    run,
                    lease,
                    "memory_syncing",
                    &format!("正在同步运行记录 {}–{}", start + 1, end),
                )
                .await?;
            }
            let records = memory_records(run, start, end)?;
            if reconciling {
                tokio::select! {
                    _ = lease.cancellation.cancelled() => return Err("剧情规划已取消。".into()),
                    result = adapter.reconcile_records(self.owner_user_id, &memory.scope, &records) => result?,
                }
            } else {
                tokio::select! {
                    _ = lease.cancellation.cancelled() => return Err("剧情规划已取消。".into()),
                    result = adapter.sync_records(self.owner_user_id, &memory.scope, records) => result?,
                }
            }
            self.require_active(lease).await?;
            let digest = synced_digest(run, end)?;
            let checkpoint = run
                .checkpoint
                .memory
                .as_mut()
                .ok_or_else(|| "剧情规划历史记录无效。".to_owned())?;
            checkpoint.synced_message_count = end;
            checkpoint.sync_in_flight_end = None;
            checkpoint.synced_digest = digest;
            self.persist(
                run,
                lease,
                "memory_synced",
                &format!("已同步 {end} 条运行记录"),
            )
            .await?;
        }
    }

    async fn fail_memory(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
        message: &str,
    ) -> Result<(), String> {
        run.checkpoint.status = StoryPlanningStatus::Failed;
        run.checkpoint.stop_reason = Some(message.to_owned());
        self.persist(
            run,
            lease,
            "memory_failed",
            "历史服务暂时不可用，运行记录已保存",
        )
        .await
    }

    async fn resolve_model(&self, run: &StoryPlanningRun) -> Result<ResolvedStoryModel, String> {
        let model_id = run.draft.models.text_model_id.clone();
        let mut values = self
            .authentication
            .resolved_models_for_owner(self.owner_user_id, vec![model_id.clone()])
            .await
            .map_err(|error| error.message)?;
        self.require_current().await?;
        if values.len() != 1 {
            return Err("所选文字模型已不可用。".into());
        }
        let model = resolved_model(self.owner_user_id, &model_id, values.remove(0))?;
        self.authentication
            .save_model_credential(self.owner_user_id, &model_id, &model.api_key)?;
        self.require_current().await?;
        Ok(model)
    }

    async fn apply_completed(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
    ) -> Result<StoryPlanningOutcome, String> {
        self.require_active(lease).await?;
        let store = self.manager.store();
        let owner = self.owner_user_id.to_owned();
        let apply_run = run.clone();
        let now = current_unix_milliseconds();
        let (applied, project) = tauri::async_runtime::spawn_blocking(move || {
            store.apply_planning_run(&owner, &apply_run, now)
        })
        .await
        .map_err(|_| "剧情规划应用任务异常退出。".to_owned())??;
        self.require_active(lease).await?;
        *run = applied;
        self.publish(run, lease)?;
        Ok(StoryPlanningOutcome {
            run: run.snapshot(self.owner_user_id),
            project: Some(StoryProjectSnapshot::from_project(&project)),
            stop_reason: None,
        })
    }

    async fn fail_model(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
        message: &str,
    ) -> Result<(), String> {
        run.checkpoint.status = StoryPlanningStatus::Failed;
        run.checkpoint.stop_reason = Some(safe_provider_error(message));
        self.persist(
            run,
            lease,
            "model_request_failed",
            "模型请求失败，运行记录已保存",
        )
        .await?;
        Err(run
            .checkpoint
            .stop_reason
            .clone()
            .unwrap_or_else(|| "模型请求失败。".into()))
    }

    async fn stop<T>(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
        status: StoryPlanningStatus,
        reason: &str,
        kind: &str,
    ) -> Result<T, String> {
        run.checkpoint.status = status;
        run.checkpoint.stop_reason = Some(reason.into());
        self.persist(run, lease, kind, reason).await?;
        Err(reason.into())
    }

    async fn persist(
        &self,
        run: &mut StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
        kind: &str,
        detail: &str,
    ) -> Result<(), String> {
        self.require_active(lease).await?;
        run.updated_at_unix_ms = current_unix_milliseconds();
        run.events.push(StoryPlanningEvent {
            id: uuid::Uuid::new_v4().to_string(),
            date_unix_ms: run.updated_at_unix_ms,
            kind: kind.into(),
            detail: clipped(detail, 1_000),
            model_calls: run.checkpoint.model_calls,
        });
        let store = self.manager.store();
        let owner = self.owner_user_id.to_owned();
        let value = run.clone();
        tauri::async_runtime::spawn_blocking(move || store.save_planning_run(&owner, &value))
            .await
            .map_err(|_| "剧情规划保存任务异常退出。".to_owned())??;
        self.require_active(lease).await?;
        self.publish(run, lease)
    }

    fn publish(
        &self,
        run: &StoryPlanningRun,
        lease: &StoryPlanningExecutionLease,
    ) -> Result<(), String> {
        if !self.manager.planning_executions.accepts(lease) {
            return Err("剧情规划执行身份已经变化。".into());
        }
        self.app
            .emit(
                STORY_PLANNING_EVENT,
                StoryPlanningEventEnvelope {
                    presentation_id: self.presentation_id.to_owned(),
                    run: run.snapshot(self.owner_user_id),
                },
            )
            .map_err(|_| "剧情规划状态发布失败。".into())
    }

    async fn require_active(&self, lease: &StoryPlanningExecutionLease) -> Result<(), String> {
        self.require_current().await?;
        if lease.cancellation.is_cancelled() || !self.manager.planning_executions.accepts(lease) {
            return Err("剧情规划已被页面关闭、账号切换或更新的请求取消。".into());
        }
        Ok(())
    }

    async fn require_current(&self) -> Result<(), String> {
        self.authentication
            .require_session_generation(self.owner_user_id, self.authentication_generation)
            .await
    }
}

pub(super) async fn abandon_run(
    authentication: &AuthenticationManager,
    manager: &StoryStudioManager,
    owner_user_id: &str,
    authentication_generation: u64,
    project_id: &str,
    run_id: &str,
) -> Result<StoryPlanningRunSnapshot, String> {
    authentication
        .require_session_generation(owner_user_id, authentication_generation)
        .await?;
    manager.planning_executions.cancel_run(
        owner_user_id,
        authentication_generation,
        project_id,
        run_id,
    );
    let store = manager.store();
    let owner = owner_user_id.to_owned();
    let project_id = project_id.to_owned();
    let run_id = run_id.to_owned();
    let run = tauri::async_runtime::spawn_blocking(move || {
        let mut run = store.load_planning_run(&owner, &project_id, &run_id)?;
        if run.applied || run.abandoned_at_unix_ms.is_some() {
            return Err("剧情规划运行已经结束，不能重复放弃。".into());
        }
        let now = current_unix_milliseconds();
        run.abandoned_at_unix_ms = Some(now);
        run.checkpoint.stop_reason = Some("用户已放弃这份中断草稿；正式项目未被修改。".into());
        run.updated_at_unix_ms = now;
        run.events.push(StoryPlanningEvent {
            id: uuid::Uuid::new_v4().to_string(),
            date_unix_ms: now,
            kind: "abandoned".into(),
            detail: "用户放弃中断草稿，正式项目保持不变".into(),
            model_calls: run.checkpoint.model_calls,
        });
        store.save_planning_run(&owner, &run)?;
        Ok::<_, String>(run)
    })
    .await
    .map_err(|_| "剧情规划放弃任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(owner_user_id, authentication_generation)
        .await?;
    Ok(run.snapshot(owner_user_id))
}

#[derive(Clone)]
struct ResolvedStoryModel {
    base_url: String,
    api_key: String,
    model: String,
    provider: String,
    supports_responses: bool,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    thinking_level: Option<String>,
}

fn resolved_model(
    owner_user_id: &str,
    expected_model_id: &str,
    model: GatewayModelConfig,
) -> Result<ResolvedStoryModel, String> {
    if model.id != expected_model_id
        || model.owner_user_id != owner_user_id
        || !model.enabled
        || !model.has_api_key
        || model.model.trim().is_empty()
        || model.provider.trim().is_empty()
    {
        return Err("所选文字模型已不可用。".into());
    }
    let api_key = model
        .api_key
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "文字模型没有可用凭据。".to_owned())?;
    let base_url = model
        .base_url
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "文字模型没有 API 地址。".to_owned())?;
    let parsed = reqwest::Url::parse(&base_url).map_err(|_| "文字模型 API 地址无效。")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err("文字模型 API 地址无效。".into());
    }
    Ok(ResolvedStoryModel {
        base_url,
        api_key,
        model: model.model,
        provider: model.provider,
        supports_responses: model.supports_responses,
        temperature: model.temperature,
        max_output_tokens: model.max_output_tokens,
        thinking_level: model.task_thinking_level,
    })
}

pub(super) fn checkpoint_input(messages: &[StoryAgentMessage]) -> Result<Vec<Value>, String> {
    let mut items = Vec::new();
    for message in messages {
        match message.role {
            StoryAgentMessageRole::System => {}
            StoryAgentMessageRole::User => items.push(json!({
                "type": "message", "role": "user", "content": message.content
            })),
            StoryAgentMessageRole::Assistant => {
                if let Some(bytes) = &message.response_output_json {
                    let output: Vec<Value> = serde_json::from_slice(bytes)
                        .map_err(|_| "剧情规划模型历史无效。".to_owned())?;
                    items.extend(output);
                } else if !message.content.trim().is_empty() {
                    items.push(json!({
                        "type": "message", "role": "assistant", "content": message.content
                    }));
                }
                if message.response_output_json.is_none() {
                    items.extend(message.tool_calls.iter().map(|call| {
                        build_function_call_item(&call.id, &call.name, &call.arguments)
                    }));
                }
            }
            StoryAgentMessageRole::Tool => {
                let call_id = message
                    .tool_call_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "剧情规划工具历史无效。".to_owned())?;
                items.push(build_function_call_output_item(call_id, &message.content));
            }
        }
    }
    Ok(items)
}

fn parse_story_tool_calls(value: &Value) -> Result<Vec<StoryAgentToolCall>, String> {
    parse_tool_calls_value(value)
        .iter()
        .map(|call| {
            let id = extract_tool_call_id(call)
                .map(str::to_owned)
                .ok_or_else(|| "模型工具调用缺少 ID。".to_owned())?;
            let name = extract_tool_call_name(call)
                .map(str::to_owned)
                .ok_or_else(|| "模型工具调用缺少名称。".to_owned())?;
            Ok(StoryAgentToolCall {
                id,
                name,
                arguments: tool_call_arguments_text(call),
            })
        })
        .collect()
}

fn prepare_for_resume(run: &mut StoryPlanningRun) -> Result<(), String> {
    if run.applied || run.abandoned_at_unix_ms.is_some() {
        return Err("剧情规划运行已经结束，不能恢复。".into());
    }
    if let Some(in_flight) = run.checkpoint.in_flight_call_id.clone() {
        let call = run
            .checkpoint
            .pending_calls
            .iter()
            .find(|value| value.id == in_flight)
            .ok_or_else(|| "剧情规划运行记录无效。".to_owned())?;
        if !tools::definitions(run.stage)
            .iter()
            .any(|definition| definition.name == call.name)
        {
            return Err("剧情规划运行记录无效。".into());
        }
        if let Some(receipt) = run.tool_receipts.get(&in_flight) {
            if receipt.name != call.name || receipt.arguments != call.arguments {
                return Err("剧情规划运行记录无效。".into());
            }
            run.checkpoint
                .receipts
                .insert(in_flight.clone(), receipt.outcome.clone());
        }
        run.checkpoint.in_flight_call_id = None;
    }
    run.checkpoint.no_progress_rounds = 0;
    run.policy = StoryAgentRunPolicy::default();
    Ok(())
}

fn call_fingerprint(call: &StoryAgentToolCall) -> String {
    let normalized = serde_json::from_str::<Value>(&call.arguments)
        .ok()
        .and_then(|value| serde_json::to_string(&value).ok())
        .unwrap_or_else(|| call.arguments.clone());
    format!(
        "{:x}",
        Sha256::digest(format!("{}:{normalized}", call.name).as_bytes())
    )
}

fn usage_from_value(value: &Value) -> StoryAgentUsage {
    let input_tokens = usage_number(value, &["input_tokens", "prompt_tokens"]);
    let output_tokens = usage_number(value, &["output_tokens", "completion_tokens"]);
    let cached_tokens = value
        .get("input_tokens_details")
        .or_else(|| value.get("prompt_tokens_details"))
        .and_then(|details| usage_number_option(details, &["cached_tokens"]))
        .unwrap_or_default();
    StoryAgentUsage {
        input_tokens,
        cached_tokens,
        output_tokens,
        requests: 1,
    }
}

fn usage_number(value: &Value, keys: &[&str]) -> u64 {
    usage_number_option(value, keys).unwrap_or_default()
}

fn usage_number_option(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn safe_provider_error(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("401") || lower.contains("403") || lower.contains("unauthorized") {
        "文字模型鉴权失败，请检查模型配置。".into()
    } else if lower.contains("429") || lower.contains("rate") {
        "文字模型请求过于频繁，请稍后恢复运行。".into()
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "文字模型请求超时，检查点已保存，可稍后恢复。".into()
    } else {
        "文字模型请求失败，检查点已保存，可在确认模型服务后恢复。".into()
    }
}

fn clipped(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

fn current_unix_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| value.as_millis().try_into().ok())
        .unwrap_or_default()
}

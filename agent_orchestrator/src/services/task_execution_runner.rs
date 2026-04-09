use std::time::Duration;

use dashmap::DashSet;
use once_cell::sync::Lazy;
use tracing::{info, warn};

use crate::config::Config;
use crate::services::im_task_runtime_bridge::publish_task_runtime_update_best_effort;
use crate::services::memory_server_client;
use crate::services::runtime_guidance_manager::runtime_guidance_manager;
use crate::services::task_service_client::{
    self, AckAllDoneRequestDto, SchedulerRequestDto, TaskExecutionScopeDto,
    TaskRecordDto, UpdateTaskRequestDto,
};

mod result_sync;
mod runtime;

use result_sync::{
    compact_result_summary, persist_task_handoff, save_task_notice_message, sync_task_result_brief,
};
use runtime::{build_task_runtime, is_task_context_overflow_error};

static ACTIVE_SCOPE_JOBS: Lazy<DashSet<String>> = Lazy::new(DashSet::new);

pub fn start() {
    if !Config::get().task_scheduler_enabled {
        info!("[TASK-RUNNER] disabled by config");
        return;
    }

    tokio::spawn(async move {
        let interval_secs = Config::get().task_scheduler_interval_secs.max(1) as u64;
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            dispatch_tick().await;
        }
    });
}

async fn dispatch_tick() {
    let limit = Some(Config::get().task_scheduler_scope_limit.max(1));
    let scopes = match task_service_client::list_scheduler_scopes(None, limit).await {
        Ok(items) => items,
        Err(err) => {
            warn!("[TASK-RUNNER] list scopes failed: {}", err);
            return;
        }
    };

    for scope in scopes {
        if ACTIVE_SCOPE_JOBS.insert(scope.scope_key.clone()) {
            tokio::spawn(async move {
                let scope_key = scope.scope_key.clone();
                let result = memory_server_client::with_internal_scope(process_scope(scope)).await;
                if let Err(err) = result {
                    warn!("[TASK-RUNNER] scope processing failed: {}", err);
                }
                ACTIVE_SCOPE_JOBS.remove(&scope_key);
            });
        }
    }
}

async fn process_scope(scope: TaskExecutionScopeDto) -> Result<(), String> {
    let decision = task_service_client::scheduler_next(&SchedulerRequestDto {
        user_id: Some(scope.user_id.clone()),
        contact_agent_id: scope.contact_agent_id.clone(),
        project_id: scope.project_id.clone(),
    })
    .await?;

    match decision.decision.as_str() {
        "task" => {
            let task = decision
                .task
                .ok_or_else(|| format!("scope {} missing task payload", scope.scope_key))?;
            execute_task(scope, task).await
        }
        "all_done" => handle_all_done(scope).await,
        "await_resume" => Ok(()),
        "pass" => Ok(()),
        other => Err(format!(
            "scope {} returned unsupported decision {}",
            scope.scope_key, other
        )),
    }
}

async fn execute_task(scope: TaskExecutionScopeDto, task: TaskRecordDto) -> Result<(), String> {
    info!(
        "[TASK-RUNNER] execute task start: scope={} task_id={} session_id={}",
        scope.scope_key,
        task.id,
        task.session_id.as_deref().unwrap_or("")
    );
    publish_task_runtime_update_best_effort(&task).await;

    let mut task_runtime = match build_task_runtime(scope.clone(), Some(&task)).await {
        Ok(runtime) => runtime,
        Err(err) => return fail_task(scope, task, err.as_str()).await,
    };
    let chat_options = task_runtime.chat_options(task.id.as_str(), Some(&task));
    let runtime_turn_id = chat_options
        .turn_id
        .clone()
        .unwrap_or_else(|| format!("task-exec-{}", task.id));
    runtime_guidance_manager().register_active_turn(
        task_runtime.runtime_session_key.as_str(),
        runtime_turn_id.as_str(),
    );
    let result = task_runtime
        .ai_server
        .chat(
            task_runtime.runtime_session_key.as_str(),
            task.content.as_str(),
            chat_options,
        )
        .await;
    runtime_guidance_manager().close_turn(
        task_runtime.runtime_session_key.as_str(),
        runtime_turn_id.as_str(),
    );

    let result = match result {
        Err(err) if is_task_context_overflow_error(err.as_str()) => {
            warn!(
                "[TASK-RUNNER] context overflow detected, forcing task execution summary before retry: scope={} task_id={} error={}",
                scope.scope_key, task.id, err
            );
            match memory_server_client::run_task_execution_summary_once_for_scope(
                scope.user_id.as_str(),
                scope.contact_agent_id.as_str(),
                scope.project_id.as_str(),
            )
            .await
            {
                Ok(_) => {
                    let mut retry_runtime = match build_task_runtime(scope.clone(), Some(&task)).await
                    {
                        Ok(runtime) => runtime,
                        Err(rebuild_err) => {
                            return fail_task(
                                scope,
                                task,
                                format!(
                                    "{}; rebuild after forced summary failed: {}",
                                    err, rebuild_err
                                )
                                .as_str(),
                            )
                            .await;
                        }
                    };
                    let retry_chat_options =
                        retry_runtime.chat_options(format!("{}-retry-overflow", task.id).as_str(), Some(&task));
                    let retry_turn_id = retry_chat_options
                        .turn_id
                        .clone()
                        .unwrap_or_else(|| format!("task-exec-{}-retry-overflow", task.id));
                    runtime_guidance_manager().register_active_turn(
                        retry_runtime.runtime_session_key.as_str(),
                        retry_turn_id.as_str(),
                    );
                    let retry_result = retry_runtime
                        .ai_server
                        .chat(
                            retry_runtime.runtime_session_key.as_str(),
                            task.content.as_str(),
                            retry_chat_options,
                        )
                        .await;
                    runtime_guidance_manager().close_turn(
                        retry_runtime.runtime_session_key.as_str(),
                        retry_turn_id.as_str(),
                    );
                    retry_result
                }
                Err(summary_err) => Err(format!(
                    "{}; force task execution summary failed: {}",
                    err, summary_err
                )),
            }
        }
        other => other,
    };

    match result {
        Ok(payload) => {
            if let Some(latest_task) = task_service_client::get_task(task.id.as_str()).await? {
                if latest_task.status != "running" {
                    info!(
                        "[TASK-RUNNER] skip auto-complete because task already transitioned: scope={} task_id={} status={}",
                        scope.scope_key, task.id, latest_task.status
                    );
                    return Ok(());
                }
            }
            let final_text = payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let saved_notice = match save_task_notice_message(
                task.session_id.as_deref(),
                "task_execution_notice",
                "completed",
                &scope,
                Some(&task),
                if final_text.is_empty() {
                    format!("任务“{}”已完成。", task.title)
                } else {
                    format!("任务“{}”已完成。\n\n{}", task.title, final_text)
                },
            )
            .await
            {
                Ok(message) => message,
                Err(err) => {
                    warn!(
                        "[TASK-RUNNER] save completion notice failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, err
                    );
                    None
                }
            };

            let result_summary = compact_result_summary(
                if final_text.is_empty() {
                    format!("任务“{}”已完成", task.title)
                } else {
                    final_text.clone()
                }
                .as_str(),
            );
            let updated_task = match task_service_client::update_task_internal(
                task.id.as_str(),
                &UpdateTaskRequestDto {
                    status: Some("completed".to_string()),
                    result_summary: Some(Some(result_summary.clone())),
                    result_message_id: Some(
                        saved_notice
                            .as_ref()
                            .map(|m| Some(m.id.clone()))
                            .unwrap_or(None),
                    ),
                    last_error: Some(None),
                    ..UpdateTaskRequestDto::default()
                },
            )
            .await
            {
                Ok(task) => task,
                Err(err) => {
                    warn!(
                        "[TASK-RUNNER] update completed status failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, err
                    );
                    None
                }
            };
            if let Some(updated_task) = updated_task.as_ref() {
                publish_task_runtime_update_best_effort(updated_task).await;
                let result_brief = match sync_task_result_brief(
                    &scope,
                    updated_task,
                    "completed",
                    result_summary.as_str(),
                    saved_notice.as_ref().map(|item| item.id.as_str()),
                )
                .await
                {
                    Ok(item) => item,
                    Err(err) => {
                        warn!(
                            "[TASK-RUNNER] sync completion brief failed: scope={} task_id={} error={}",
                            scope.scope_key, task.id, err
                        );
                        None
                    }
                };
                if let Err(err) = persist_task_handoff(
                    &scope,
                    updated_task,
                    "completed",
                    result_summary.as_str(),
                    saved_notice.as_ref().map(|item| item.id.as_str()),
                    result_brief.as_ref(),
                    None,
                    None,
                )
                .await
                {
                    warn!(
                        "[TASK-RUNNER] persist completion handoff failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, err
                    );
                }
            } else {
                warn!(
                    "[TASK-RUNNER] update completed status failed: scope={} task_id={} error={}",
                    scope.scope_key, task.id, "task not returned after update"
                );
            }
            info!(
                "[TASK-RUNNER] execute task completed: scope={} task_id={}",
                scope.scope_key, task.id
            );
            Ok(())
        }
        Err(err) => {
            if let Some(latest_task) = task_service_client::get_task(task.id.as_str()).await? {
                if latest_task.status != "running" {
                    info!(
                        "[TASK-RUNNER] skip auto-fail because task already transitioned: scope={} task_id={} status={}",
                        scope.scope_key, task.id, latest_task.status
                    );
                    return Ok(());
                }
            }
            if let Err(notice_err) = save_task_notice_message(
                task.session_id.as_deref(),
                "task_execution_notice",
                "failed",
                &scope,
                Some(&task),
                format!("任务“{}”执行失败：{}", task.title, err),
            )
            .await
            {
                warn!(
                    "[TASK-RUNNER] save failure notice failed: scope={} task_id={} error={}",
                    scope.scope_key, task.id, notice_err
                );
            }
            let updated_task = match task_service_client::update_task_internal(
                task.id.as_str(),
                &UpdateTaskRequestDto {
                    status: Some("failed".to_string()),
                    result_summary: Some(Some(compact_result_summary(err.as_str()))),
                    result_message_id: Some(None),
                    last_error: Some(Some(err.clone())),
                    ..UpdateTaskRequestDto::default()
                },
            )
            .await
            {
                Ok(task) => task,
                Err(update_err) => {
                    warn!(
                        "[TASK-RUNNER] update failed status failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, update_err
                    );
                    None
                }
            };
            if let Some(updated_task) = updated_task.as_ref() {
                publish_task_runtime_update_best_effort(updated_task).await;
                let result_brief =
                    match sync_task_result_brief(&scope, updated_task, "failed", err.as_str(), None)
                        .await
                    {
                        Ok(item) => item,
                        Err(bridge_err) => {
                            warn!(
                                "[TASK-RUNNER] sync failure brief failed: scope={} task_id={} error={}",
                                scope.scope_key, task.id, bridge_err
                            );
                            None
                        }
                    };
                if let Err(bridge_err) = persist_task_handoff(
                    &scope,
                    updated_task,
                    "failed",
                    err.as_str(),
                    None,
                    result_brief.as_ref(),
                    Some(err.as_str()),
                    None,
                )
                .await
                {
                    warn!(
                        "[TASK-RUNNER] persist failure handoff failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, bridge_err
                    );
                }
            } else {
                warn!(
                    "[TASK-RUNNER] update failed status failed: scope={} task_id={} error={}",
                    scope.scope_key, task.id, "task not returned after update"
                );
            }
            Err(format!(
                "scope {} task {} execution failed: {}",
                scope.scope_key, task.id, err
            ))
        }
    }
}

async fn fail_task(
    scope: TaskExecutionScopeDto,
    task: TaskRecordDto,
    err: &str,
) -> Result<(), String> {
    if let Some(latest_task) = task_service_client::get_task(task.id.as_str()).await? {
        if latest_task.status != "running" {
            info!(
                "[TASK-RUNNER] skip setup-failure write because task already transitioned: scope={} task_id={} status={}",
                scope.scope_key, task.id, latest_task.status
            );
            return Ok(());
        }
    }
    if let Err(notice_err) = save_task_notice_message(
        task.session_id.as_deref(),
        "task_execution_notice",
        "failed",
        &scope,
        Some(&task),
        format!("任务“{}”执行失败：{}", task.title, err),
    )
    .await
    {
        warn!(
            "[TASK-RUNNER] save setup-failure notice failed: scope={} task_id={} error={}",
            scope.scope_key, task.id, notice_err
        );
    }
    let updated_task = match task_service_client::update_task_internal(
        task.id.as_str(),
        &UpdateTaskRequestDto {
            status: Some("failed".to_string()),
            result_summary: Some(Some(compact_result_summary(err))),
            result_message_id: Some(None),
            last_error: Some(Some(err.to_string())),
            ..UpdateTaskRequestDto::default()
        },
    )
    .await
    {
        Ok(task) => task,
        Err(update_err) => {
            warn!(
                "[TASK-RUNNER] update setup-failure status failed: scope={} task_id={} error={}",
                scope.scope_key, task.id, update_err
            );
            None
        }
    };
    if let Some(updated_task) = updated_task.as_ref() {
        publish_task_runtime_update_best_effort(updated_task).await;
        let result_brief = match sync_task_result_brief(&scope, updated_task, "failed", err, None)
            .await
        {
            Ok(item) => item,
            Err(bridge_err) => {
                warn!(
                    "[TASK-RUNNER] sync setup-failure brief failed: scope={} task_id={} error={}",
                    scope.scope_key, task.id, bridge_err
                );
                None
            }
        };
        if let Err(bridge_err) = persist_task_handoff(
            &scope,
            updated_task,
            "failed",
            err,
            None,
            result_brief.as_ref(),
            Some(err),
            None,
        )
        .await
        {
            warn!(
                "[TASK-RUNNER] persist setup-failure handoff failed: scope={} task_id={} error={}",
                scope.scope_key, task.id, bridge_err
            );
        }
    } else {
        warn!(
            "[TASK-RUNNER] update setup-failure status failed: scope={} task_id={} error={}",
            scope.scope_key, task.id, "task not returned after update"
        );
    }
    Err(format!(
        "scope {} task {} execution failed: {}",
        scope.scope_key, task.id, err
    ))
}

async fn handle_all_done(scope: TaskExecutionScopeDto) -> Result<(), String> {
    let Some(session_id) = scope
        .latest_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
    else {
        task_service_client::ack_all_done(&AckAllDoneRequestDto {
            user_id: Some(scope.user_id.clone()),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            ack_at: None,
        })
        .await?;
        return Ok(());
    };

    let mut task_runtime = build_task_runtime(scope.clone(), None).await?;
    let summary_prompt = "当前这个联系人的后台任务都已经执行完成。请基于已有任务执行记录，给用户一段简短、自然的结语：说明任务已全部完成，并概括最终结果；不要输出过程推理，不要编造未完成事项。";
    let chat_options = task_runtime.chat_options("all_done", None);
    let runtime_turn_id = chat_options
        .turn_id
        .clone()
        .unwrap_or_else(|| "task-exec-all_done".to_string());
    runtime_guidance_manager().register_active_turn(
        task_runtime.runtime_session_key.as_str(),
        runtime_turn_id.as_str(),
    );
    let result = task_runtime
        .ai_server
        .chat(
            task_runtime.runtime_session_key.as_str(),
            summary_prompt,
            chat_options,
        )
        .await;
    runtime_guidance_manager().close_turn(
        task_runtime.runtime_session_key.as_str(),
        runtime_turn_id.as_str(),
    );
    let result = result?;
    let final_text = result
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("后台任务已全部执行完成。")
        .trim()
        .to_string();

    if let Err(err) = save_task_notice_message(
        Some(session_id.as_str()),
        "task_execution_notice",
        "all_done",
        &scope,
        None,
        if final_text.is_empty() {
            "后台任务已全部执行完成。".to_string()
        } else {
            final_text
        },
    )
    .await
    {
        warn!(
            "[TASK-RUNNER] save all-done notice failed: scope={} error={}",
            scope.scope_key, err
        );
    }

    task_service_client::ack_all_done(&AckAllDoneRequestDto {
        user_id: Some(scope.user_id.clone()),
        contact_agent_id: scope.contact_agent_id.clone(),
        project_id: scope.project_id.clone(),
        ack_at: None,
    })
    .await?;
    Ok(())
}

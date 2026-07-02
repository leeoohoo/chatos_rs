// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn log_run_model_phase_start(
    run: &TaskRunRecord,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
    input: &StartTaskRunRequest,
    effective_workspace_dir: &str,
) {
    info!(
        run_id = run.id.as_str(),
        task_id = task.id.as_str(),
        task_title = task.title.as_str(),
        model_config_id = model_config.id.as_str(),
        model = model_config.model.as_str(),
        provider = model_config.provider.as_str(),
        workspace_dir = effective_workspace_dir,
        prompt_override = input.prompt_override.as_deref().unwrap_or(""),
        "task runner begin execute_run"
    );
}

pub(super) async fn initialize_model_phase(
    service: &RunService,
    task: &TaskRecord,
    run: &mut TaskRunRecord,
    effective_workspace_dir: &str,
    prerequisite_context: &[PrerequisiteTaskContext],
) -> bool {
    if service.store.is_cancel_requested(&run.id)
        || service
            .store
            .get_run(&run.id)
            .await
            .ok()
            .flatten()
            .is_some_and(|current| current.status == TaskRunStatus::Cancelled)
        || service
            .store
            .get_task(&task.id)
            .await
            .ok()
            .flatten()
            .is_some_and(|current| current.status == TaskStatus::Cancelled)
    {
        service
            .finish_cancelled_before_start(task, run, effective_workspace_dir)
            .await;
        return false;
    }

    if !mark_run_running(service, run).await {
        return false;
    }
    mark_task_running(service, task, &run.id).await;
    persist_prerequisite_context(service, run, prerequisite_context).await;
    service
        .ensure_task_terminal_started(task, run, effective_workspace_dir)
        .await;
    true
}

async fn mark_run_running(service: &RunService, run: &mut TaskRunRecord) -> bool {
    run.status = TaskRunStatus::Running;
    if run.started_at.is_none() {
        run.started_at = Some(now_rfc3339());
    }
    run.updated_at = now_rfc3339();
    match service.store.save_run(run.clone()).await {
        Ok(saved) => {
            *run = saved;
        }
        Err(err) => {
            warn!("failed to persist running task run {}: {}", run.id, err);
            return false;
        }
    }
    if let Err(err) = service
        .store
        .append_run_event(TaskRunEventRecord::new(
            run.id.clone(),
            "running",
            Some("任务开始执行".to_string()),
            None,
        ))
        .await
    {
        warn!("failed to append running event for run {}: {}", run.id, err);
    }
    true
}

async fn mark_task_running(service: &RunService, task: &TaskRecord, run_id: &str) {
    if let Ok(Some(mut task_record)) = service.store.get_task(&task.id).await {
        if task_record.status == TaskStatus::Cancelled {
            return;
        }
        task_record.status = TaskStatus::Running;
        task_record.updated_at = now_rfc3339();
        task_record.last_run_id = Some(run_id.to_string());
        if let Err(err) = service.store.save_task(task_record).await {
            warn!("failed to persist running task {}: {}", task.id, err);
        }
    }
}

async fn persist_prerequisite_context(
    service: &RunService,
    run: &mut TaskRunRecord,
    prerequisite_context: &[PrerequisiteTaskContext],
) {
    if prerequisite_context.is_empty() {
        return;
    }

    attach_prerequisite_context_to_run(run, prerequisite_context);
    run.updated_at = now_rfc3339();
    if let Err(err) = service.store.save_run(run.clone()).await {
        warn!(
            "failed to persist prerequisite context for run {}: {}",
            run.id, err
        );
    }
}

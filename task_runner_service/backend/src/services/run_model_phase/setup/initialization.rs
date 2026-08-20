// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn initialize_model_phase(
    service: &RunService,
    task: &TaskRecord,
    run: &mut TaskRunRecord,
    effective_workspace_dir: &str,
    prerequisite_context: &[PrerequisiteTaskContext],
    authoritative_policy: bool,
) -> Result<bool, String> {
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
        return Ok(false);
    }

    let mut entered_execution_lane = false;
    for _ in 0..3 {
        if wait_for_execution_lane(service, run).await? {
            return Err(crate::services::CLOUD_AGENT_DEPENDENCY_WAITING.to_string());
        }
        match mark_run_running(service, run).await {
            Ok(()) => {
                entered_execution_lane = true;
                break;
            }
            Err(error) if error == crate::store::EXECUTION_LANE_BUSY_ERROR => continue,
            Err(error) => return Err(error),
        }
    }
    if !entered_execution_lane {
        if wait_for_execution_lane(service, run).await? {
            return Err(crate::services::CLOUD_AGENT_DEPENDENCY_WAITING.to_string());
        }
        return Err(crate::store::EXECUTION_LANE_BUSY_ERROR.to_string());
    }
    if mark_task_running(service, task, &run.id).await {
        service.try_send_started_callback(run).await;
    }
    persist_prerequisite_context(service, run, prerequisite_context).await;
    let _ = (effective_workspace_dir, authoritative_policy);
    Ok(true)
}

async fn wait_for_execution_lane(
    service: &RunService,
    run: &TaskRunRecord,
) -> Result<bool, String> {
    let Some(execution_lane_key) = run.execution_lane_key.as_deref() else {
        return Ok(false);
    };
    let Some(active_run) = service
        .store
        .get_running_run_for_execution_lane(execution_lane_key, run.id.as_str())
        .await?
    else {
        return Ok(false);
    };
    let current = service
        .store
        .subscribe_run_terminal(crate::store::RunTerminalSubscriptionRecord::cloud_agent(
            active_run.id.as_str(),
            run.id.as_str(),
        ))
        .await?;
    if matches!(
        current.status,
        TaskRunStatus::Succeeded
            | TaskRunStatus::Failed
            | TaskRunStatus::Cancelled
            | TaskRunStatus::Blocked
    ) {
        return Ok(false);
    }
    if let Err(error) = service
        .store
        .append_run_event(TaskRunEventRecord::new(
            run.id.clone(),
            "execution_lane_waiting_event",
            Some("等待同项目上一任务完成终态收敛".to_string()),
            Some(json!({
                "execution_lane_key": execution_lane_key,
                "active_run_id": active_run.id,
            })),
        ))
        .await
    {
        warn!(
            run_id = run.id.as_str(),
            active_run_id = active_run.id.as_str(),
            error = error.as_str(),
            "failed to append execution lane waiting event"
        );
    }
    Ok(true)
}

async fn mark_run_running(service: &RunService, run: &mut TaskRunRecord) -> Result<(), String> {
    let mut candidate = run.clone();
    candidate.status = TaskRunStatus::Running;
    candidate.model_phase_status = crate::models::ModelPhaseStatus::Running;
    if candidate.started_at.is_none() {
        candidate.started_at = Some(now_rfc3339());
    }
    candidate.updated_at = now_rfc3339();
    match service.store.save_run(candidate).await {
        Ok(saved) => {
            *run = saved;
        }
        Err(err) => {
            warn!("failed to persist running task run {}: {}", run.id, err);
            return Err(err);
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
    Ok(())
}

async fn mark_task_running(service: &RunService, task: &TaskRecord, run_id: &str) -> bool {
    if let Ok(Some(mut task_record)) = service.store.get_task(&task.id).await {
        if task_record.status == TaskStatus::Cancelled {
            return false;
        }
        task_record.status = TaskStatus::Running;
        task_record.updated_at = now_rfc3339();
        task_record.last_run_id = Some(run_id.to_string());
        if let Err(err) = service.store.save_task(task_record).await {
            warn!("failed to persist running task {}: {}", task.id, err);
            return false;
        }
        return true;
    }
    false
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

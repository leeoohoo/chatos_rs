// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use serde_json::{json, Value};
use tracing::warn;

use super::{
    MEMORY_CLOUD_AGENT_DEFERRED, THREAD_RESUMMARIZE_JOB_RESUME_KIND, THREAD_SUMMARY_JOB_RESUME_KIND,
};
use crate::state::AppState;

pub(crate) async fn finalize_terminal(
    state: &Arc<AppState>,
    run: &CloudAgentRunRecord,
) -> Result<(), String> {
    let terminal_context = run
        .terminal_outcome
        .as_ref()
        .and_then(|value| value.get("terminal_context"))
        .cloned()
        .or_else(|| run.input.get("terminal_context").cloned())
        .unwrap_or_default();
    if terminal_context.is_null() {
        return Err("Memory Cloud Agent terminal context is missing".to_string());
    }
    let resume_kind = terminal_context
        .get("resume_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "Memory Cloud Agent resume_kind is missing".to_string())?;
    let tenant_id = terminal_context
        .get("tenant_id")
        .and_then(Value::as_str)
        .unwrap_or(run.owner_user_id.as_str());
    let source_id = terminal_context
        .get("source_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let thread_id = terminal_context
        .get("thread_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if run.status != chatos_cloud_agent_protocol::CloudAgentRunStatus::Succeeded {
        let error = run
            .terminal_outcome
            .as_ref()
            .and_then(|value| value.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("Memory Cloud Agent failed")
            .to_string();
        if resume_kind == THREAD_RESUMMARIZE_JOB_RESUME_KIND {
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Memory resummarization terminal context has no job_run_id".to_string()
                })?;
            crate::services::summary::fail_covered_range_resummarize(
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
                error,
            )
            .await?;
        } else if is_thread_summary_resume_kind(resume_kind) {
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Memory summary terminal context has no job_run_id".to_string())?;
            let dispatch_error = error.clone();
            let dead_lettered = crate::services::summary::fail_cloud_summary_job(
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
                error,
            )
            .await?;
            if dead_lettered {
                warn!(
                    thread_id,
                    job_run_id,
                    error = dispatch_error,
                    "Memory Engine owned partition summary dispatch was dead-lettered"
                );
            }
        } else {
            finalize_failed_domain_job(state, &terminal_context, error).await?;
        }
        release_subject_scope_slot(state, &terminal_context).await?;
        return Ok(());
    }

    match resume_kind {
        THREAD_RESUMMARIZE_JOB_RESUME_KIND => {
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Memory resummarization terminal context has no job_run_id".to_string()
                })?;
            let result = crate::services::summary::resume_covered_range_resummarize(
                &state.config,
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
            )
            .await;
            match result {
                Ok(_) => Ok(()),
                Err(error) if error == MEMORY_CLOUD_AGENT_DEFERRED => Ok(()),
                Err(error) => Err(error),
            }
        }
        THREAD_SUMMARY_JOB_RESUME_KIND => {
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Memory summary terminal context has no job_run_id".to_string())?;
            let result = crate::services::summary::resume_cloud_summary_job(
                &state.config,
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
            )
            .await;
            match result {
                Ok(Some(_)) => {
                    settle_summary_dispatch(
                        state,
                        tenant_id,
                        source_id,
                        thread_id,
                        terminal_context
                            .get("project_partition_key")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                "Memory summary terminal context has no partition".to_string()
                            })?,
                    )
                    .await
                }
                Ok(None) => Ok(()),
                Err(error) if error == MEMORY_CLOUD_AGENT_DEFERRED => Ok(()),
                Err(error) => Err(error),
            }
        }
        "subject_memory_job" => {
            let request = serde_json::from_value::<crate::models::RunSubjectMemoryJobRequest>(
                terminal_context
                    .get("request")
                    .cloned()
                    .ok_or_else(|| "subject memory terminal request is missing".to_string())?,
            )
            .map_err(|error| format!("decode subject memory resume request failed: {error}"))?;
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "subject memory terminal job_run_id is missing".to_string())?;
            let generation_id = terminal_context
                .get("generation_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "subject memory terminal generation_id is missing".to_string())?;
            let from_scope_runner = terminal_context
                .get("from_scope_runner")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let result = crate::services::subject_memory::resume_subject_memory_job(
                &state.config,
                &state.pool,
                request,
                from_scope_runner,
                job_run_id,
                generation_id,
                terminal_context
                    .get("scope_lock_owner")
                    .and_then(Value::as_str),
            )
            .await;
            match result {
                Ok(_) => Ok(()),
                Err(error) if error == MEMORY_CLOUD_AGENT_DEFERRED => Ok(()),
                Err(error) => Err(error),
            }
        }
        "rollup_job" => {
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "rollup terminal job_run_id is missing".to_string())?;
            let settings = crate::services::control_plane::load_rollup_settings(&state.config)?;
            let descriptor = terminal_context
                .get("generation")
                .ok_or_else(|| "rollup terminal generation descriptor is missing".to_string())?;
            let input_ids = descriptor
                .get("input_manifest")
                .and_then(|value| value.get("inputs"))
                .and_then(Value::as_array)
                .ok_or_else(|| "rollup terminal frozen inputs are missing".to_string())?
                .iter()
                .map(|input| {
                    input
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                        .ok_or_else(|| "rollup terminal frozen input id is invalid".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let policy = descriptor
                .get("policy_snapshot")
                .ok_or_else(|| "rollup terminal policy snapshot is missing".to_string())?;
            let level = policy
                .get("source_level")
                .and_then(Value::as_i64)
                .ok_or_else(|| "rollup terminal source_level is missing".to_string())?;
            let trigger_reason = policy
                .get("trigger_reason")
                .and_then(Value::as_str)
                .ok_or_else(|| "rollup terminal trigger_reason is missing".to_string())?;
            let project_partition_key = descriptor
                .get("project_partition_key")
                .and_then(Value::as_str)
                .ok_or_else(|| "rollup terminal project partition is missing".to_string())?;
            let selected = crate::repositories::summaries::list_summaries_by_ids(
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                input_ids.as_slice(),
            )
            .await?;
            let prepared = crate::services::summary::PreparedThreadRollup {
                thread_id: thread_id.to_string(),
                project_partition_key: project_partition_key.to_string(),
                level,
                selected,
                trigger_reason: trigger_reason.to_string(),
                cloud_job_run_id: Some(job_run_id.to_string()),
            };
            let result = crate::services::summary::run_prepared_thread_rollup(
                &state.config,
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                prepared,
                &settings,
                "queue",
            )
            .await;
            match result {
                Ok(_) => Ok(()),
                Err(error) if error == MEMORY_CLOUD_AGENT_DEFERRED => Ok(()),
                Err(error) => Err(error),
            }
        }
        other => Err(format!(
            "unsupported Memory Cloud Agent resume_kind: {other}"
        )),
    }
}

pub(super) fn is_thread_summary_resume_kind(resume_kind: &str) -> bool {
    resume_kind == THREAD_SUMMARY_JOB_RESUME_KIND
}

async fn settle_summary_dispatch(
    state: &Arc<AppState>,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<(), String> {
    crate::repositories::thread_partitions::consume_current_summary_dispatch(
        &state.pool,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
    )
    .await
    .map(|_| ())
}

async fn finalize_failed_domain_job(
    state: &Arc<AppState>,
    terminal_context: &Value,
    error: String,
) -> Result<(), String> {
    let Some(job_run_id) = terminal_context.get("job_run_id").and_then(Value::as_str) else {
        return Ok(());
    };
    crate::repositories::control_plane::finish_job_run(
        &state.pool,
        job_run_id,
        crate::models::FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: 0,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: 1,
            metadata: Some(json!({"cloud_agent_terminal_failure": true})),
            error_message: Some(error),
        },
    )
    .await?;
    if terminal_context.get("resume_kind").and_then(Value::as_str) == Some("rollup_job") {
        let required = |key: &str| {
            terminal_context
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("failed rollup terminal {key} is missing"))
        };
        crate::repositories::thread_partitions::release_rollup_claim(
            &state.pool,
            required("tenant_id")?,
            required("source_id")?,
            required("thread_id")?,
            required("project_partition_key")?,
            job_run_id,
        )
        .await?;
    }
    Ok(())
}

async fn release_subject_scope_slot(
    state: &Arc<AppState>,
    terminal_context: &Value,
) -> Result<(), String> {
    let Some(lock_owner) = terminal_context
        .get("scope_lock_owner")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let request = terminal_context
        .get("request")
        .cloned()
        .ok_or_else(|| "subject memory terminal request is missing".to_string())?;
    let request = serde_json::from_value::<crate::models::RunSubjectMemoryJobRequest>(request)
        .map_err(|error| format!("decode subject memory terminal request failed: {error}"))?;
    let scope_key = terminal_context
        .get("scope_key")
        .and_then(Value::as_str)
        .or(request.scope_key.as_deref())
        .ok_or_else(|| "subject memory terminal scope_key is missing".to_string())?;
    crate::repositories::subject_memory_scopes::release_subject_memory_scope_slot(
        &state.pool,
        request.tenant_id.as_str(),
        request.source_id.as_str(),
        scope_key,
        lock_owner,
    )
    .await
}

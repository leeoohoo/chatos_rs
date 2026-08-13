// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_agent::{AgentIdentity, MemoryEngineAgent};
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use chatos_cloud_agent_runtime::{
    create_cloud_agent_run, CloudAgentRunStore, CloudAgentStateStore, NewCloudAgentRun,
};
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::db::Db;
use crate::services::ai_pipeline::cloud_agent::{
    CloudSummaryPipelineSpec, CloudSummaryPipelineState,
};
use crate::state::AppState;

pub(crate) const MEMORY_CLOUD_AGENT_DEFERRED: &str = "memory_cloud_agent_deferred";

#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_or_defer(
    config: &AppConfig,
    db: &Db,
    store: &CloudAgentStateStore,
    agent: &MemoryEngineAgent,
    owner_user_id: Option<&str>,
    ordering_lane_key: String,
    owner_entity_type: &str,
    owner_entity_id: &str,
    spec: CloudSummaryPipelineSpec,
    terminal_context: Value,
) -> Result<crate::services::ai_pipeline::SummaryBuildResult, String> {
    let agent_run_id = format!(
        "memory_agent:{}:{}",
        agent.descriptor().key,
        owner_entity_id
    );
    if let Some(run) = store.load_run(agent_run_id.as_str()).await? {
        return completed_result_or_deferred(&run);
    }

    refresh_subject_scope_slot(config, db, &terminal_context).await?;

    let runtime = crate::services::control_plane::build_managed_memory_agent_runtime(
        config,
        db,
        agent,
        owner_user_id,
    )
    .await?;
    if !runtime.ai.is_enabled() {
        return Err(format!(
            "{} model is not configured or enabled",
            agent.job_type()
        ));
    }
    let input = crate::cloud_agent_queue::MemoryCloudAgentInput {
        pipeline: CloudSummaryPipelineState::new(CloudSummaryPipelineSpec {
            summary_prompt: Some(runtime.prompt),
            ..spec
        })?,
        terminal_context,
    };
    let input = serde_json::to_value(input).map_err(|error| error.to_string())?;
    let created = create_cloud_agent_run(
        store,
        NewCloudAgentRun {
            ordering_lane_key,
            agent_run_id: agent_run_id.clone(),
            owner_service: "memory-engine".to_string(),
            owner_entity_type: owner_entity_type.to_string(),
            owner_entity_id: owner_entity_id.to_string(),
            owner_user_id: owner_user_id.unwrap_or("system").to_string(),
            agent_key: agent.descriptor().key.as_str().to_string(),
            input,
            model_config_ref: runtime.model_profile_id.clone(),
            model_runtime_snapshot_ref: runtime.model_profile_id,
            agent_prompt_revision: runtime.prompt_revision.to_string(),
            agent_prompt_checksum: runtime.prompt_checksum,
            capability_policy_revision: "tool_plane_none".to_string(),
            mcp_runtime_session_ref: None,
            current_input_items_ref: format!("memory_agent:{owner_entity_id}:input"),
            max_iterations: 1024,
            deadline_at: None,
            runtime_routing_key: crate::cloud_agent_queue::MEMORY_CLOUD_AGENT_ROUTING_KEY
                .to_string(),
            start_causation_id: owner_entity_id.to_string(),
            start_payload: json!({"owner_entity_id": owner_entity_id}),
        },
    )
    .await;
    match created {
        Ok(_) => Err(MEMORY_CLOUD_AGENT_DEFERRED.to_string()),
        Err(error) => {
            if let Some(run) = store.load_run(agent_run_id.as_str()).await? {
                completed_result_or_deferred(&run)
            } else {
                Err(error)
            }
        }
    }
}

async fn refresh_subject_scope_slot(
    config: &AppConfig,
    db: &Db,
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
        .ok_or_else(|| "subject memory Cloud Agent request is missing".to_string())?;
    let request = serde_json::from_value::<crate::models::RunSubjectMemoryJobRequest>(request)
        .map_err(|error| format!("decode subject memory Cloud Agent request failed: {error}"))?;
    let scope_key = terminal_context
        .get("scope_key")
        .and_then(Value::as_str)
        .or(request.scope_key.as_deref())
        .ok_or_else(|| "subject memory Cloud Agent scope_key is missing".to_string())?;
    let lock_timeout_secs = cloud_subject_scope_lock_timeout_secs(config);
    let refreshed = crate::repositories::subject_memory_scopes::refresh_subject_memory_scope_slot(
        db,
        request.tenant_id.as_str(),
        request.source_id.as_str(),
        scope_key,
        lock_owner,
        lock_timeout_secs,
    )
    .await?;
    if !refreshed {
        return Err(
            "subject memory scope ownership was lost before Cloud Agent creation".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn cloud_subject_scope_lock_timeout_secs(config: &AppConfig) -> i64 {
    let model_window = config
        .ai_request_timeout_secs
        .saturating_mul(2)
        .saturating_add(60);
    config
        .subject_memory_lock_timeout_secs
        .max(i64::try_from(model_window).unwrap_or(i64::MAX))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_or_defer_from_config(
    config: &AppConfig,
    db: &Db,
    agent: &MemoryEngineAgent,
    owner_user_id: Option<&str>,
    ordering_lane_key: String,
    owner_entity_type: &str,
    owner_entity_id: &str,
    spec: CloudSummaryPipelineSpec,
    terminal_context: Value,
) -> Result<crate::services::ai_pipeline::SummaryBuildResult, String> {
    let store = CloudAgentStateStore::connect_to_database(
        config.mongodb_uri.as_str(),
        config.mongodb_database.as_str(),
    )
    .await?;
    generate_or_defer(
        config,
        db,
        &store,
        agent,
        owner_user_id,
        ordering_lane_key,
        owner_entity_type,
        owner_entity_id,
        spec,
        terminal_context,
    )
    .await
}

fn completed_result_or_deferred(
    run: &CloudAgentRunRecord,
) -> Result<crate::services::ai_pipeline::SummaryBuildResult, String> {
    if !run.status.is_terminal() {
        return Err(MEMORY_CLOUD_AGENT_DEFERRED.to_string());
    }
    let outcome = run.terminal_outcome.as_ref().cloned().unwrap_or_default();
    if run.status != chatos_cloud_agent_protocol::CloudAgentRunStatus::Succeeded {
        return Err(outcome
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Memory Cloud Agent failed")
            .to_string());
    }
    let build = outcome
        .get("summary_build")
        .ok_or_else(|| "Memory Cloud Agent terminal result is missing summary_build".to_string())?;
    Ok(crate::services::ai_pipeline::SummaryBuildResult {
        text: build
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "Memory Cloud Agent result text is missing".to_string())?
            .to_string(),
        chunk_count: build
            .get("chunk_count")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(1),
        overflow_retry_count: build
            .get("overflow_retry_count")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0),
    })
}

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
        finalize_failed_domain_job(state, &terminal_context, error).await?;
        release_subject_scope_slot(state, &terminal_context).await?;
        return Ok(());
    }

    match resume_kind {
        "queue" | "summary_direct" | "scheduler" | "thread_direct" => {
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
                Ok(_) => Ok(()),
                Err(error) if error == MEMORY_CLOUD_AGENT_DEFERRED => Ok(()),
                Err(error) => Err(error),
            }
        }
        "thread_repair_direct" => {
            let result = crate::services::summary::resume_thread_repair_summary(
                &state.config,
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                terminal_context
                    .get("job_run_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .await;
            match result {
                Ok(_) => Ok(()),
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
                terminal_context
                    .get("scope_lock_owner")
                    .and_then(Value::as_str),
            )
            .await;
            match result {
                Ok(_) => release_subject_scope_slot(state, &terminal_context).await,
                Err(error) if error == MEMORY_CLOUD_AGENT_DEFERRED => Ok(()),
                Err(error) => Err(error),
            }
        }
        "rollup_job" => {
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "rollup terminal job_run_id is missing".to_string())?;
            let policy =
                crate::repositories::control_plane::get_effective_job_policy(&state.pool, "rollup")
                    .await?;
            let settings =
                crate::services::control_plane::build_rollup_settings_from_policy(&policy);
            let Some(mut prepared) = crate::services::summary::prepare_thread_rollup(
                &state.pool,
                tenant_id,
                source_id,
                thread_id,
                &settings,
            )
            .await?
            else {
                return Ok(());
            };
            prepared.cloud_job_run_id = Some(job_run_id.to_string());
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
    .await
    .map(|_| ())
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

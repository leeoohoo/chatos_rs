// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_agent::{AgentIdentity, MemoryEngineAgent};
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use chatos_cloud_agent_runtime::{
    create_cloud_agent_run, CloudAgentRunStore, CloudAgentStateStore, NewCloudAgentRun,
};
use serde_json::{json, Value};
use tracing::warn;

use crate::config::AppConfig;
use crate::db::Db;
use crate::services::ai_pipeline::cloud_agent::{
    CloudSummaryPipelineSpec, CloudSummaryPipelineState,
};
use crate::state::AppState;

pub(crate) const MEMORY_CLOUD_AGENT_DEFERRED: &str = "memory_cloud_agent_deferred";
pub(crate) const THREAD_SUMMARY_JOB_RESUME_KIND: &str = "thread_summary_job";
pub(crate) const THREAD_RESUMMARIZE_JOB_RESUME_KIND: &str = "thread_resummarize_job";

#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_or_defer(
    config: &AppConfig,
    db: &Db,
    store: &CloudAgentStateStore,
    agent: &MemoryEngineAgent,
    owner_user_id: &str,
    ordering_lane_key: String,
    owner_entity_type: &str,
    owner_entity_id: &str,
    spec: CloudSummaryPipelineSpec,
    mut terminal_context: Value,
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
    let payload_contract = prepare_generation_from_terminal_context(
        db,
        &mut terminal_context,
        &runtime,
        &spec,
        owner_entity_id,
    )
    .await?;
    let input = crate::cloud_agent_queue::MemoryCloudAgentInput {
        pipeline: CloudSummaryPipelineState::new(
            CloudSummaryPipelineSpec {
                managed_prompt: Some(runtime.prompt),
                ..spec
            },
            payload_contract,
        )?,
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
            owner_user_id: owner_user_id.to_string(),
            agent_key: agent.descriptor().key.as_str().to_string(),
            input,
            model_config_ref: runtime.model_config_id.clone(),
            model_runtime_snapshot_ref: runtime.model_config_id,
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

async fn prepare_generation_from_terminal_context(
    db: &Db,
    terminal_context: &mut Value,
    runtime: &crate::services::control_plane::ManagedMemoryAgentRuntime,
    spec: &CloudSummaryPipelineSpec,
    job_run_id: &str,
) -> Result<crate::models::structured_payload::MemoryPayloadContract, String> {
    let Some(descriptor) = terminal_context.get("generation").cloned() else {
        return Err("Memory generation descriptor is required before model execution".to_string());
    };
    let field = |name: &str| {
        descriptor
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("Memory generation descriptor.{name} is required"))
    };
    let generation_step_key = field("generation_step_key")?;
    let generation_kind = field("generation_kind")?;
    let generation_profile = field("generation_profile")?;
    let pipeline_revision = field("pipeline_revision")?;
    if pipeline_revision != spec.pipeline_revision {
        return Err(format!(
            "Memory generation pipeline revision mismatch: descriptor={pipeline_revision}, loaded={}",
            spec.pipeline_revision
        ));
    }
    let input_digest = field("input_digest")?;
    let input_manifest = descriptor
        .get("input_manifest")
        .ok_or_else(|| "Memory generation descriptor.input_manifest is required".to_string())?;
    let policy_snapshot = descriptor
        .get("policy_snapshot")
        .ok_or_else(|| "Memory generation descriptor.policy_snapshot is required".to_string())?;
    let policy_checksum = crate::services::generation_identity::sha256_json(policy_snapshot)?;
    let pipeline_checksum =
        crate::services::generation_identity::sha256_json(&serde_json::json!({
            "pipeline_revision": spec.pipeline_revision,
            "system_prompt": crate::ai::SUMMARY_SYSTEM_PROMPT,
            "leaf_directive": spec.leaf_directive,
            "merge_directive": spec.merge_directive,
        }))?;
    let config_snapshot = serde_json::json!({
        "generation_profile": generation_profile,
        "prompt_revision": runtime.prompt_revision,
        "prompt_checksum": runtime.prompt_checksum,
        "pipeline_revision": pipeline_revision,
        "pipeline_checksum": pipeline_checksum,
        "model_provider": runtime.model_provider,
        "model_id": runtime.model_id,
        "model_config_id": runtime.model_config_id,
        "tokenizer_id": crate::services::generation_identity::TOKENIZER_ID,
        "policy_checksum": policy_checksum,
        "policy_snapshot": policy_snapshot,
    });
    let generation = crate::repositories::generation_commits::prepare_generation(
        db,
        crate::repositories::generation_commits::PrepareGeneration {
            tenant_id: terminal_context
                .get("tenant_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Memory generation tenant_id is required".to_string())?,
            source_id: terminal_context
                .get("source_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Memory generation source_id is required".to_string())?,
            job_run_id,
            generation_step_key,
            generation_kind,
            input_digest,
            input_manifest,
            config_snapshot: &config_snapshot,
            project_partition_key: descriptor
                .get("project_partition_key")
                .and_then(Value::as_str),
        },
    )
    .await?;
    terminal_context["generation_id"] = serde_json::json!(generation.generation_id);
    let payload_scope = terminal_context
        .get("payload_scope")
        .and_then(Value::as_object)
        .ok_or_else(|| "Memory generation payload_scope is required".to_string())?;
    let scope_field = |name: &str| {
        payload_scope
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("Memory generation payload_scope.{name} is required"))
    };
    let source_refs = input_manifest
        .get("inputs")
        .and_then(Value::as_array)
        .ok_or_else(|| "Memory generation input_manifest.inputs is required".to_string())?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| "Memory generation input id is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut supersession_candidates = Vec::new();
    collect_fact_ids(input_manifest, &mut supersession_candidates);
    supersession_candidates.sort();
    supersession_candidates.dedup();
    let contract = crate::models::structured_payload::MemoryPayloadContract {
        generation_id: generation.generation_id,
        scope_kind: scope_field("kind")?,
        subject_id: scope_field("subject_id")?,
        project_partition_key: payload_scope
            .get("project_partition_key")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_refs,
        supersession_candidates,
        allow_empty_facts: generation_kind == "thread_summary"
            && generation_step_key.starts_with("thread-l0:"),
    };
    contract.validate()?;
    Ok(contract)
}

fn collect_fact_ids(value: &Value, result: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(fact_id) = object.get("fact_id").and_then(Value::as_str) {
                result.push(fact_id.to_string());
            }
            for value in object.values() {
                collect_fact_ids(value, result);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_fact_ids(value, result);
            }
        }
        _ => {}
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
    owner_user_id: &str,
    ordering_lane_key: String,
    owner_entity_type: &str,
    owner_entity_id: &str,
    spec: CloudSummaryPipelineSpec,
    terminal_context: Value,
) -> Result<crate::services::ai_pipeline::SummaryBuildResult, String> {
    let store = CloudAgentStateStore::from_repository(
        crate::repositories::cloud_agent::CloudAgentPostgresStore::new(db.clone()),
    );
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
        structured_payload: build
            .get("structured_payload")
            .cloned()
            .ok_or_else(|| "Memory Cloud Agent result structured_payload is missing".to_string())?,
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
        if resume_kind == THREAD_RESUMMARIZE_JOB_RESUME_KIND {
            let job_run_id = terminal_context
                .get("job_run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Memory resummarization terminal context has no job_run_id".to_string())?;
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
                .ok_or_else(|| "Memory resummarization terminal context has no job_run_id".to_string())?;
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
                Ok(_) => {
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
            let policy =
                crate::repositories::control_plane::get_effective_job_policy(&state.pool, "rollup")
                    .await?;
            let settings =
                crate::services::control_plane::build_rollup_settings_from_policy(&policy)?;
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

fn is_thread_summary_resume_kind(resume_kind: &str) -> bool {
    resume_kind == THREAD_SUMMARY_JOB_RESUME_KIND
}

#[cfg(test)]
mod tests {
    use super::{is_thread_summary_resume_kind, THREAD_SUMMARY_JOB_RESUME_KIND};

    #[test]
    fn thread_summary_resume_route_is_not_a_scheduler_trigger() {
        assert!(is_thread_summary_resume_kind(
            THREAD_SUMMARY_JOB_RESUME_KIND
        ));
        assert!(!is_thread_summary_resume_kind("database_worker"));
        assert!(!is_thread_summary_resume_kind("scheduler"));
        assert!(!is_thread_summary_resume_kind("thread_direct"));
    }
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

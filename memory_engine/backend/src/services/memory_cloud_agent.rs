// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "memory_cloud_agent_terminal.rs"]
mod terminal;

pub(crate) use terminal::finalize_terminal;
#[cfg(test)]
use terminal::is_thread_summary_resume_kind;

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
                token_limit: runtime.budget.input_limit()?,
                ..spec
            },
            payload_contract,
            runtime.budget,
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
        "model_budget": runtime.budget,
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
    let scope_kind = scope_field("kind")?;
    let subject_id = scope_field("subject_id")?;
    let project_partition_key = payload_scope
        .get("project_partition_key")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut supersession_candidates = Vec::new();
    collect_matching_scope_fact_ids(
        input_manifest,
        scope_kind.as_str(),
        subject_id.as_str(),
        project_partition_key.as_deref(),
        &mut supersession_candidates,
    );
    supersession_candidates.sort();
    supersession_candidates.dedup();
    let contract = crate::models::structured_payload::MemoryPayloadContract {
        generation_id: generation.generation_id,
        scope_kind,
        subject_id,
        project_partition_key,
        source_refs,
        supersession_candidates,
        allow_empty_facts: generation_kind == "thread_summary"
            && generation_step_key.starts_with("thread-l0:"),
    };
    contract.validate()?;
    Ok(contract)
}

fn collect_matching_scope_fact_ids(
    value: &Value,
    scope_kind: &str,
    subject_id: &str,
    project_partition_key: Option<&str>,
    result: &mut Vec<String>,
) {
    match value {
        Value::Object(object) => {
            if object.get("schema_version").and_then(Value::as_str)
                == Some(crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA)
                && payload_scope_matches(
                    object.get("scope"),
                    scope_kind,
                    subject_id,
                    project_partition_key,
                )
            {
                for collection in [
                    "completed",
                    "decisions",
                    "open_items",
                    "blockers",
                    "artifacts",
                    "constraints",
                    "stable_preferences",
                    "uncertainties",
                ] {
                    if let Some(facts) = object.get(collection).and_then(Value::as_array) {
                        result.extend(facts.iter().filter_map(|fact| {
                            fact.get("fact_id")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        }));
                    }
                }
                return;
            }
            for value in object.values() {
                collect_matching_scope_fact_ids(
                    value,
                    scope_kind,
                    subject_id,
                    project_partition_key,
                    result,
                );
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_matching_scope_fact_ids(
                    value,
                    scope_kind,
                    subject_id,
                    project_partition_key,
                    result,
                );
            }
        }
        _ => {}
    }
}

fn payload_scope_matches(
    scope: Option<&Value>,
    scope_kind: &str,
    subject_id: &str,
    project_partition_key: Option<&str>,
) -> bool {
    let Some(scope) = scope.and_then(Value::as_object) else {
        return false;
    };
    let payload_partition = match scope.get("project_partition_key") {
        Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.as_str()),
        _ => return false,
    };
    scope.get("kind").and_then(Value::as_str) == Some(scope_kind)
        && scope.get("subject_id").and_then(Value::as_str) == Some(subject_id)
        && payload_partition == project_partition_key
}

pub(crate) async fn refresh_subject_scope_slot(
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
            "subject memory scope ownership was lost before Cloud Agent execution".to_string(),
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        collect_matching_scope_fact_ids, is_thread_summary_resume_kind,
        THREAD_SUMMARY_JOB_RESUME_KIND,
    };

    #[test]
    fn thread_summary_resume_route_is_not_a_scheduler_trigger() {
        assert!(is_thread_summary_resume_kind(
            THREAD_SUMMARY_JOB_RESUME_KIND
        ));
        assert!(!is_thread_summary_resume_kind("database_worker"));
        assert!(!is_thread_summary_resume_kind("scheduler"));
        assert!(!is_thread_summary_resume_kind("thread_direct"));
    }

    #[test]
    fn supersession_candidates_are_restricted_to_the_output_scope() {
        let input = json!({
            "inputs": [{
                "fact_id": "untrusted-recursive-id",
                "structured_payload": {
                    "schema_version": "memory_payload_v1",
                    "scope": {
                        "kind": "thread",
                        "subject_id": "agent:test",
                        "project_partition_key": "project:alpha"
                    },
                    "completed": [{"fact_id": "thread-fact"}],
                    "decisions": [], "open_items": [], "blockers": [],
                    "artifacts": [], "constraints": [],
                    "stable_preferences": [], "uncertainties": []
                }
            }, {
                "structured_payload": {
                    "schema_version": "memory_payload_v1",
                    "scope": {
                        "kind": "project",
                        "subject_id": "project:alpha",
                        "project_partition_key": "project:alpha"
                    },
                    "completed": [],
                    "decisions": [{"fact_id": "project-fact"}],
                    "open_items": [], "blockers": [], "artifacts": [],
                    "constraints": [], "stable_preferences": [], "uncertainties": []
                }
            }]
        });
        let mut candidates = Vec::new();

        collect_matching_scope_fact_ids(
            &input,
            "project",
            "project:alpha",
            Some("project:alpha"),
            &mut candidates,
        );

        assert_eq!(candidates, vec!["project-fact"]);
    }
}

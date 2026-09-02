// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use chatos_cloud_agent_protocol::{CloudAgentRunRecord, CloudAgentRunStatus};
use chatos_cloud_agent_runtime::{
    cloud_agent_trigger_execution_identity, CloudAgentModelTrigger, CloudAgentProfile,
    CloudAgentProfileRegistry, CloudAgentRabbitMqTopology, CloudAgentServiceRuntime,
    CloudAgentSingleStepExecution, CloudAgentSingleStepOutput,
};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::services::ai_pipeline::cloud_agent::CloudSummaryPipelineState;
use crate::state::AppState;

pub(crate) const MEMORY_CLOUD_AGENT_ROUTING_KEY: &str = "cloud_agent.memory_engine.runtime";
pub(crate) const MEMORY_CLOUD_AGENT_RETRY_ROUTING_KEY: &str =
    "cloud_agent.memory_engine.runtime.retry";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct MemoryCloudAgentInput {
    pub pipeline: CloudSummaryPipelineState,
    #[serde(default)]
    pub terminal_context: Value,
}

#[derive(Clone)]
struct MemoryCloudAgentProfile {
    state: Arc<AppState>,
}

#[async_trait::async_trait]
impl CloudAgentProfile for MemoryCloudAgentProfile {
    async fn execute_single_step(
        &self,
        run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        let input = serde_json::from_value::<MemoryCloudAgentInput>(run.input.clone())
            .map_err(|error| format!("decode Memory Cloud Agent input failed: {error}"))?;
        refresh_domain_slots(&self.state, &input.terminal_context).await?;
        let ai = crate::services::control_plane::build_ai_client_for_profile_id(
            &self.state.config,
            &self.state.pool,
            run.model_config_ref.as_str(),
            run.owner_user_id.as_str(),
        )
        .await?;
        let (_, model_attempt) = cloud_agent_trigger_execution_identity(trigger);
        let (outcome, pipeline, result) = input.pipeline.execute_one(&ai, model_attempt).await?;
        let next_input = MemoryCloudAgentInput {
            pipeline,
            terminal_context: input.terminal_context,
        };
        let overlay = result.map(|result| {
            json!({
                "summary_build": {
                    "text": result.text,
                    "chunk_count": result.chunk_count,
                    "overflow_retry_count": result.overflow_retry_count,
                },
                "terminal_context": next_input.terminal_context,
            })
        });
        Ok(CloudAgentSingleStepExecution::Apply(
            CloudAgentSingleStepOutput::new(outcome)
                .with_next_input(
                    serde_json::to_value(next_input).map_err(|error| error.to_string())?,
                )
                .with_terminal_outcome_overlay(overlay),
        ))
    }

    async fn finalize_terminal(&self, run: &CloudAgentRunRecord) -> Result<(), String> {
        if run.status != CloudAgentRunStatus::Succeeded {
            warn!(
                agent_run_id = run.ordering.agent_run_id.as_str(),
                status = ?run.status,
                terminal_outcome = ?run.terminal_outcome,
                "Memory Cloud Agent ended without a generated summary"
            );
        }
        crate::services::memory_cloud_agent::finalize_terminal(&self.state, run).await
    }
}

async fn refresh_domain_slots(state: &AppState, terminal_context: &Value) -> Result<(), String> {
    refresh_summary_slot(state, terminal_context).await?;
    refresh_subject_scope_slot(state, terminal_context).await
}

async fn refresh_summary_slot(state: &AppState, terminal_context: &Value) -> Result<(), String> {
    let is_summary = matches!(
        terminal_context.get("resume_kind").and_then(Value::as_str),
        Some("queue" | "summary_direct" | "scheduler" | "thread_direct")
    );
    if !is_summary {
        return Ok(());
    }
    let tenant_id = terminal_context
        .get("tenant_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "summary Cloud Agent tenant_id is missing".to_string())?;
    let source_id = terminal_context
        .get("source_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "summary Cloud Agent source_id is missing".to_string())?;
    let thread_id = terminal_context
        .get("thread_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "summary Cloud Agent thread_id is missing".to_string())?;
    let job_run_id = terminal_context
        .get("job_run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "summary Cloud Agent job_run_id is missing".to_string())?;
    let lock_timeout_secs = state
        .config
        .ai_request_timeout_secs
        .saturating_mul(2)
        .saturating_add(60);
    let refreshed = crate::repositories::threads::refresh_summary_slot(
        &state.pool,
        tenant_id,
        source_id,
        thread_id,
        job_run_id,
        i64::try_from(lock_timeout_secs).unwrap_or(i64::MAX),
    )
    .await?;
    if !refreshed {
        return Err("summary slot ownership was lost before Cloud Agent execution".to_string());
    }
    Ok(())
}

async fn refresh_subject_scope_slot(
    state: &AppState,
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
    let refreshed = crate::repositories::subject_memory_scopes::refresh_subject_memory_scope_slot(
        &state.pool,
        request.tenant_id.as_str(),
        request.source_id.as_str(),
        scope_key,
        lock_owner,
        crate::services::memory_cloud_agent::cloud_subject_scope_lock_timeout_secs(&state.config),
    )
    .await?;
    if !refreshed {
        return Err(
            "subject memory scope ownership was lost before Cloud Agent execution".to_string(),
        );
    }
    Ok(())
}

fn runtime(state: Arc<AppState>) -> CloudAgentServiceRuntime<CloudAgentProfileRegistry> {
    let registry = CloudAgentProfileRegistry::new("memory-engine", state.cloud_agent_store.clone())
        .register(
            [
                SystemAgentKey::MemoryEngineSummaryAgent.as_str(),
                SystemAgentKey::MemoryEngineRollupAgent.as_str(),
                SystemAgentKey::MemoryEngineSubjectMemoryAgent.as_str(),
                SystemAgentKey::MemoryEngineMemoryRollupAgent.as_str(),
                SystemAgentKey::MemoryEngineThreadRepairAgent.as_str(),
            ],
            MemoryCloudAgentProfile { state },
        )
        .expect("Memory Engine Cloud Agent profiles must be valid");
    CloudAgentServiceRuntime::new(registry, MEMORY_CLOUD_AGENT_ROUTING_KEY)
}

fn topology(state: &AppState) -> CloudAgentRabbitMqTopology {
    CloudAgentRabbitMqTopology {
        rabbitmq_url: state.config.rabbitmq_url.clone(),
        exchange: state.config.rabbitmq_exchange.clone(),
        runtime_queue: MEMORY_CLOUD_AGENT_ROUTING_KEY.to_string(),
        retry_queue: MEMORY_CLOUD_AGENT_RETRY_ROUTING_KEY.to_string(),
        consumer_tag: "memory-engine-cloud-agent-runtime".to_string(),
        reconnect_delay: state.config.rabbitmq_reconnect_delay,
        outbox_reconcile_interval: Duration::from_secs(1),
        outbox_batch_size: 100,
        prefetch_count: 32,
        consumer_concurrency: 4,
        conflict_retry_delay: Duration::from_secs(1),
    }
}

pub(crate) fn start(state: Arc<AppState>) {
    chatos_cloud_agent_runtime::spawn_cloud_agent_outbox_reconciler(
        topology(&state),
        runtime(state.clone()),
    );
    chatos_cloud_agent_runtime::spawn_cloud_agent_consumer(topology(&state), runtime(state));
}

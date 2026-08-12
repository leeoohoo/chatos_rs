// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_cloud_agent_runtime::{
    publish_cloud_agent_intent, CloudAgentOutboxIntent, CloudAgentRabbitMqTopology,
    CloudAgentRunStore, CloudAgentServiceRuntime,
};
use tokio::task::JoinHandle;

use crate::platform_queue::TaskQueueTopology;
use crate::services::RunService;

pub(crate) const TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY: &str = "cloud_agent.task_runner.runtime";
pub(crate) const TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY: &str =
    "cloud_agent.task_runner.runtime.retry";

fn runtime(run_service: RunService) -> CloudAgentServiceRuntime<RunService> {
    CloudAgentServiceRuntime::new(run_service, TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY)
}

fn cloud_agent_topology(
    topology: &TaskQueueTopology,
) -> Result<CloudAgentRabbitMqTopology, String> {
    Ok(CloudAgentRabbitMqTopology {
        rabbitmq_url: topology.rabbitmq_url.clone().ok_or_else(|| {
            "TASK_RUNNER_RABBITMQ_URL is required for Cloud Agent orchestration".to_string()
        })?,
        exchange: topology.rabbitmq_exchange.clone(),
        runtime_queue: TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY.to_string(),
        retry_queue: TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY.to_string(),
        consumer_tag: "task-runner-cloud-agent-runtime".to_string(),
        reconnect_delay: topology.rabbitmq_reconnect_delay,
        outbox_reconcile_interval: Duration::from_secs(1),
        outbox_batch_size: 100,
        prefetch_count: 32,
        conflict_retry_delay: Duration::from_secs(1),
    })
}

pub fn spawn_cloud_agent_outbox_reconciler(
    topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    let cloud_topology = cloud_agent_topology(&topology)
        .expect("Task Runner Cloud Agent RabbitMQ topology must be configured");
    chatos_cloud_agent_runtime::spawn_cloud_agent_outbox_reconciler(
        cloud_topology,
        runtime(run_service),
    )
}

pub fn spawn_cloud_agent_consumer(
    topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    let cloud_topology = cloud_agent_topology(&topology)
        .expect("Task Runner Cloud Agent RabbitMQ topology must be configured");
    chatos_cloud_agent_runtime::spawn_cloud_agent_consumer(cloud_topology, runtime(run_service))
}

pub(crate) async fn publish_dependency_resume(
    topology: &TaskQueueTopology,
    run_service: &RunService,
    parent_run_id: &str,
    dependency_run_id: &str,
) -> Result<(), String> {
    let parent_run = run_service
        .get_run(parent_run_id)
        .await?
        .ok_or_else(|| format!("parent Task Run not found: {parent_run_id}"))?;
    let agent_run_id = parent_run
        .agent_run_id
        .as_deref()
        .ok_or_else(|| "parent Task Run has no Cloud Agent run".to_string())?;
    let cloud_run = run_service
        .cloud_agent_store()
        .load_run(agent_run_id)
        .await?
        .ok_or_else(|| "parent Cloud Agent run not found".to_string())?;
    if cloud_run.status.is_terminal() {
        return Ok(());
    }
    let intent = CloudAgentOutboxIntent {
        event_id: format!("dependency_ready:{parent_run_id}:{dependency_run_id}"),
        topic: "run_started".to_string(),
        routing_key: TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY.to_string(),
        ordering: cloud_run.ordering,
        causation_id: dependency_run_id.to_string(),
        correlation_id: agent_run_id.to_string(),
        available_at: chrono::Utc::now(),
        payload: serde_json::json!({
            "event_type": "dependency_ready",
            "task_run_id": parent_run_id,
            "dependency_run_id": dependency_run_id,
        }),
    };
    publish_cloud_agent_intent(
        &cloud_agent_topology(topology)?,
        &runtime(run_service.clone()),
        &intent,
    )
    .await
}

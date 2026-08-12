// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_cloud_agent_runtime::{CloudAgentRabbitMqTopology, CloudAgentServiceRuntime};
use tokio::task::JoinHandle;

use crate::state::AppState;

pub(crate) const PROJECT_CLOUD_AGENT_ROUTING_KEY: &str = "cloud_agent.project.runtime";
pub(crate) const PROJECT_CLOUD_AGENT_RETRY_ROUTING_KEY: &str = "cloud_agent.project.runtime.retry";

fn runtime(state: AppState) -> CloudAgentServiceRuntime<AppState> {
    CloudAgentServiceRuntime::new(state, PROJECT_CLOUD_AGENT_ROUTING_KEY)
}

fn topology(state: &AppState) -> CloudAgentRabbitMqTopology {
    let queue_namespace = state.config.mcp_result_queue_prefix.trim_end_matches('.');
    CloudAgentRabbitMqTopology {
        rabbitmq_url: state.config.mcp_result_rabbitmq_url.clone(),
        exchange: format!("{queue_namespace}.cloud_agent"),
        runtime_queue: PROJECT_CLOUD_AGENT_ROUTING_KEY.to_string(),
        retry_queue: PROJECT_CLOUD_AGENT_RETRY_ROUTING_KEY.to_string(),
        consumer_tag: "project-cloud-agent-runtime".to_string(),
        reconnect_delay: Duration::from_secs(3),
        outbox_reconcile_interval: Duration::from_secs(1),
        outbox_batch_size: 100,
        prefetch_count: 32,
        conflict_retry_delay: Duration::from_secs(1),
    }
}

pub fn spawn_cloud_agent_outbox_reconciler(state: AppState) -> JoinHandle<()> {
    chatos_cloud_agent_runtime::spawn_cloud_agent_outbox_reconciler(
        topology(&state),
        runtime(state),
    )
}

pub fn spawn_cloud_agent_consumer(state: AppState) -> JoinHandle<()> {
    chatos_cloud_agent_runtime::spawn_cloud_agent_consumer(topology(&state), runtime(state))
}

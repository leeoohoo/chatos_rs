// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use async_trait::async_trait;
use chatos_cloud_agent_protocol::{CloudAgentRunPhase, CloudAgentRunStatus};
use chatos_cloud_agent_runtime::{
    CloudAgentConsumeDisposition, CloudAgentModelTrigger, CloudAgentQueueOwner,
    CloudAgentRabbitMqTopology,
};
use tokio::task::JoinHandle;

use crate::state::AppState;

pub(crate) const PROJECT_CLOUD_AGENT_ROUTING_KEY: &str = "cloud_agent.project.runtime";
pub(crate) const PROJECT_CLOUD_AGENT_RETRY_ROUTING_KEY: &str = "cloud_agent.project.runtime.retry";

#[derive(Clone)]
struct ProjectCloudAgentOwner {
    state: AppState,
}

#[async_trait]
impl CloudAgentQueueOwner for ProjectCloudAgentOwner {
    fn owner_service(&self) -> &'static str {
        "project-service"
    }

    fn cloud_agent_store(&self) -> chatos_cloud_agent_runtime::CloudAgentStateStore {
        self.state.cloud_agent_store.clone()
    }

    async fn consume_cloud_agent_event(
        &self,
        event_id: String,
        agent_run_id: String,
        trigger: CloudAgentModelTrigger,
        expected_status: CloudAgentRunStatus,
        expected_phase: CloudAgentRunPhase,
    ) -> Result<CloudAgentConsumeDisposition, String> {
        crate::services::environment_agent::consume_cloud_agent_event(
            &self.state,
            event_id,
            agent_run_id,
            trigger,
            expected_status,
            expected_phase,
        )
        .await
    }

    async fn finalize_cloud_agent_terminal(&self, agent_run_id: &str) -> Result<(), String> {
        crate::services::environment_agent::finalize_cloud_agent_terminal(&self.state, agent_run_id)
            .await
    }
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
        ProjectCloudAgentOwner { state },
    )
}

pub fn spawn_cloud_agent_consumer(state: AppState) -> JoinHandle<()> {
    chatos_cloud_agent_runtime::spawn_cloud_agent_consumer(
        topology(&state),
        ProjectCloudAgentOwner { state },
    )
}

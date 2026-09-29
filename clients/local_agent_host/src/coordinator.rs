// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    HostRequestHandler, LocalAgentScheduler, LocalAgentSchedulerError, LocalToolScheduler,
    LocalToolSchedulerError, SchedulerTick, ToolSchedulerTick,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{HostRequestEnvelope, HostResponseEnvelope};
use chatos_local_agent_runtime::{LocalAgentRuntime, LocalAgentRuntimeError};
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::{watch, Notify};

#[derive(Debug, Error)]
pub enum LocalAgentCoordinatorError {
    #[error(transparent)]
    ModelScheduler(#[from] LocalAgentSchedulerError),
    #[error(transparent)]
    ToolScheduler(#[from] LocalToolSchedulerError),
    #[error(transparent)]
    Runtime(#[from] LocalAgentRuntimeError),
    #[error("system clock is before the Unix epoch")]
    Clock,
}

pub struct LocalAgentHostCoordinator {
    runtime: Arc<LocalAgentRuntime>,
    model_scheduler: Option<LocalAgentScheduler>,
    tool_scheduler: Option<LocalToolScheduler>,
    wakeup: Arc<Notify>,
}

impl LocalAgentHostCoordinator {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        model_scheduler: Option<LocalAgentScheduler>,
        tool_scheduler: Option<LocalToolScheduler>,
    ) -> Result<Self, String> {
        if model_scheduler.is_none() && tool_scheduler.is_none() {
            return Err("Local Agent coordinator requires a model or tool scheduler".to_string());
        }
        Ok(Self {
            runtime,
            model_scheduler,
            tool_scheduler,
            wakeup: Arc::new(Notify::new()),
        })
    }

    pub fn wake(&self) {
        self.wakeup.notify_one();
    }

    /// Runs until the watch value becomes true or all senders are dropped.
    /// Work is event-driven; the only timer is the next durable retry.
    pub async fn run_until_shutdown(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            self.drain_ready_work(&shutdown).await?;
            if *shutdown.borrow() {
                return Ok(());
            }
            let delay = self.next_retry_delay().await?;
            tokio::select! {
                _ = self.wakeup.notified() => {}
                _ = tokio::time::sleep(delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn drain_ready_work(
        &self,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            let mut progressed = false;
            if let Some(scheduler) = self.model_scheduler.as_ref() {
                progressed |= matches!(scheduler.run_once().await?, SchedulerTick::Committed(_));
            }
            if *shutdown.borrow() {
                return Ok(());
            }
            if let Some(scheduler) = self.tool_scheduler.as_ref() {
                progressed |=
                    matches!(scheduler.run_once().await?, ToolSchedulerTick::Committed(_));
            }
            if !progressed {
                return Ok(());
            }
        }
    }

    async fn next_retry_delay(&self) -> Result<Duration, LocalAgentCoordinatorError> {
        let Some(next_retry_at) = self.runtime.next_retry_at().await? else {
            return Ok(Duration::from_secs(24 * 60 * 60));
        };
        let now = system_now_unix_ms()?;
        Ok(Duration::from_millis(
            u64::try_from(next_retry_at.saturating_sub(now).max(0)).unwrap_or(0),
        ))
    }
}

#[async_trait]
impl HostRequestHandler for LocalAgentHostCoordinator {
    async fn handle_request(&self, request: HostRequestEnvelope) -> HostResponseEnvelope {
        let response = self.runtime.handle(request).await;
        if response.ok {
            self.wake();
        }
        response
    }
}

fn system_now_unix_ms() -> Result<i64, LocalAgentCoordinatorError> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| LocalAgentCoordinatorError::Clock)?;
    i64::try_from(duration.as_millis()).map_err(|_| LocalAgentCoordinatorError::Clock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        decode_response, read_frame, serve_stream, write_frame, LocalToolExecutor,
        LocalToolRegistry,
    };
    use async_trait::async_trait;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CreateRunCommand, HostCommand, HostResult, LocalAgentRunClaim,
        LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry};
    use serde_json::json;
    use uuid::Uuid;

    struct ModelProfile;

    #[async_trait]
    impl LocalAgentProfile for ModelProfile {
        async fn execute_step(
            &self,
            claim: &LocalAgentRunClaim,
        ) -> Result<chatos_local_agent_protocol::LocalAgentStepOutcome, String> {
            if claim.run.continuation_input.is_some() {
                Ok(
                    chatos_local_agent_protocol::LocalAgentStepOutcome::Succeed {
                        output: json!({"completed": true}),
                    },
                )
            } else {
                Ok(
                    chatos_local_agent_protocol::LocalAgentStepOutcome::WaitForTool {
                        batch_id: "batch-coordinator".to_string(),
                        tool_calls: vec![chatos_local_agent_protocol::LocalAgentToolCall {
                            call_id: "call-coordinator".to_string(),
                            tool_name: "read_file".to_string(),
                            arguments: json!({"path": "README.md"}),
                            side_effecting: false,
                        }],
                        checkpoint: json!({"model_step": 1}),
                    },
                )
            }
        }
    }

    struct ReadFile;

    #[async_trait]
    impl LocalToolExecutor for ReadFile {
        async fn execute_tool(
            &self,
            _invocation: &LocalAgentToolInvocationRecord,
        ) -> Result<LocalAgentToolOutcome, String> {
            Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({"content": "hello"}),
            })
        }
    }

    #[tokio::test]
    async fn coordinator_drains_model_tool_model_without_polling() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize().await.expect("initialize");
        let mut profiles = LocalAgentProfileRegistry::new();
        profiles
            .register("coordinator", ModelProfile)
            .expect("profile");
        let model_scheduler =
            LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "model-worker")
                .expect("model scheduler");
        let mut tools = LocalToolRegistry::new();
        tools.register("read_file", ReadFile).expect("tool");
        let tool_scheduler = LocalToolScheduler::new(Arc::clone(&runtime), tools, "tool-worker")
            .expect("tool scheduler");
        let coordinator = Arc::new(
            LocalAgentHostCoordinator::new(
                Arc::clone(&runtime),
                Some(model_scheduler),
                Some(tool_scheduler),
            )
            .expect("coordinator"),
        );
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move { coordinator.run_until_shutdown(shutdown_rx).await })
        };
        let (mut client, server) = tokio::io::duplex(16 * 1024);
        let ipc_task = tokio::spawn(serve_stream(server, Arc::clone(&coordinator)));
        let request = HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-coordinator".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-coordinator".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "coordinator".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "hello"}),
                max_iterations: 4,
            }),
        };
        write_frame(&mut client, &serde_json::to_vec(&request).expect("request"))
            .await
            .expect("write");
        let response = read_frame(&mut client)
            .await
            .expect("read")
            .expect("response");
        let response = decode_response(&response).expect("decode");
        assert!(response.ok);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let response = runtime
                .handle(HostRequestEnvelope {
                    protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                    command_id: format!("get-{}", Uuid::new_v4()),
                    command: HostCommand::GetRun {
                        run_id: "run-coordinator".to_string(),
                    },
                })
                .await;
            let status = match response.result.expect("run") {
                HostResult::Run { run } => run.status,
                result => panic!("unexpected result: {result:?}"),
            };
            if status == chatos_local_agent_protocol::LocalAgentRunStatus::Succeeded {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "coordinator timed out"
            );
            tokio::task::yield_now().await;
        }
        shutdown_tx.send(true).expect("shutdown");
        task.await.expect("join").expect("coordinator");
        drop(client);
        ipc_task.await.expect("IPC join").expect("IPC server");
    }
}

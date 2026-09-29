// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    HostRequestHandler, LocalAgentScheduler, LocalAgentSchedulerError, LocalToolScheduler,
    LocalToolSchedulerError, SchedulerTick, ToolSchedulerTick,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    validate_identifier, HostCommand, HostRequestEnvelope, HostResponseEnvelope, HostResult,
};
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
    activity: watch::Sender<u64>,
    reserved_ipc_tools: Vec<String>,
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
        let (activity, _) = watch::channel(0);
        Ok(Self {
            runtime,
            model_scheduler,
            tool_scheduler,
            wakeup: Arc::new(Notify::new()),
            activity,
            reserved_ipc_tools: Vec::new(),
        })
    }

    pub fn with_reserved_ipc_tools<I, S>(mut self, tool_names: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut names = Vec::new();
        for tool_name in tool_names {
            let tool_name = tool_name.into();
            validate_identifier("reserved_ipc_tool", &tool_name)?;
            if names.contains(&tool_name) {
                return Err(format!("reserved IPC tool is duplicated: {tool_name}"));
            }
            names.push(tool_name);
        }
        if names.len() > 128 {
            return Err("at most 128 IPC tools can be reserved".to_string());
        }
        self.reserved_ipc_tools = names;
        Ok(self)
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
                let committed = matches!(scheduler.run_once().await?, SchedulerTick::Committed(_));
                if committed {
                    self.signal_activity();
                }
                progressed |= committed;
            }
            if *shutdown.borrow() {
                return Ok(());
            }
            if let Some(scheduler) = self.tool_scheduler.as_ref() {
                let committed =
                    matches!(scheduler.run_once().await?, ToolSchedulerTick::Committed(_));
                if committed {
                    self.signal_activity();
                }
                progressed |= committed;
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

    fn signal_activity(&self) {
        self.activity
            .send_modify(|value| *value = value.wrapping_add(1));
    }

    async fn wait_for_events(
        &self,
        request: HostRequestEnvelope,
        timeout_ms: u64,
    ) -> HostResponseEnvelope {
        let mut activity = self.activity.subscribe();
        let mut response = self.runtime.handle(request.clone()).await;
        if !response.ok || response_has_events(&response) {
            return response;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => return response,
                changed = activity.changed() => {
                    if changed.is_err() {
                        return response;
                    }
                    response = self.runtime.handle(request.clone()).await;
                    if !response.ok || response_has_events(&response) {
                        return response;
                    }
                }
            }
        }
    }

    fn route_external_tool_claim(&self, request: &mut HostRequestEnvelope) {
        let HostCommand::ClaimNextTool(command) = &mut request.command else {
            return;
        };
        for tool_name in &self.reserved_ipc_tools {
            if !command.exclude_tool_names.contains(tool_name) {
                command.exclude_tool_names.push(tool_name.clone());
            }
        }
    }
}

#[async_trait]
impl HostRequestHandler for LocalAgentHostCoordinator {
    async fn handle_request(&self, mut request: HostRequestEnvelope) -> HostResponseEnvelope {
        if let HostCommand::WaitEvents(command) = &request.command {
            return self
                .wait_for_events(request.clone(), command.timeout_ms)
                .await;
        }
        self.route_external_tool_claim(&mut request);
        let response = self.runtime.handle(request).await;
        if response.ok {
            self.wake();
            self.signal_activity();
        }
        response
    }
}

fn response_has_events(response: &HostResponseEnvelope) -> bool {
    matches!(
        response.result.as_ref(),
        Some(HostResult::Events { events, .. }) if !events.is_empty()
    )
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
        ClaimNextToolCommand, CreateRunCommand, HostCommand, HostResult, LocalAgentRunClaim,
        LocalAgentToolInvocationRecord, LocalAgentToolOutcome, WaitEventsCommand,
        LOCAL_AGENT_PROTOCOL_VERSION,
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
            .expect("coordinator")
            .with_reserved_ipc_tools(["create_task"])
            .expect("reserved tools"),
        );
        let mut external_claim = HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-native-tool".to_string(),
            command: HostCommand::ClaimNextTool(ClaimNextToolCommand {
                worker_id: "native-worker".to_string(),
                lease_duration_ms: 10_000,
                include_tool_names: None,
                exclude_tool_names: Vec::new(),
            }),
        };
        coordinator.route_external_tool_claim(&mut external_claim);
        let HostCommand::ClaimNextTool(routed) = external_claim.command else {
            panic!("expected tool claim")
        };
        assert_eq!(routed.exclude_tool_names, vec!["create_task"]);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move { coordinator.run_until_shutdown(shutdown_rx).await })
        };
        let wait_task = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move {
                coordinator
                    .handle_request(HostRequestEnvelope {
                        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                        command_id: "wait-coordinator-events".to_string(),
                        command: HostCommand::WaitEvents(WaitEventsCommand {
                            after_cursor: 0,
                            limit: 50,
                            run_id: Some("run-coordinator".to_string()),
                            timeout_ms: 2_000,
                        }),
                    })
                    .await
            })
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
        let waited = wait_task.await.expect("wait join");
        assert!(response_has_events(&waited));
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

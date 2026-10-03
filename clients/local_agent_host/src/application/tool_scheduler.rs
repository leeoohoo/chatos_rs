// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    ClaimNextToolCommand, CommitToolCommand, HostCommand, HostRequestEnvelope, HostResult,
    LocalAgentToolCommitResult, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{LocalAgentRuntime, LocalAgentRuntimeError};
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use thiserror::Error;
use uuid::Uuid;

#[async_trait]
pub trait LocalToolExecutor: Send + Sync {
    /// Executes one already-persisted invocation. Implementations must return
    /// `NeedsReview` when a side effect may have happened but cannot be proven.
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String>;
}

#[derive(Clone, Default)]
pub struct LocalToolRegistry {
    executors: HashMap<String, Arc<dyn LocalToolExecutor>>,
}

impl LocalToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<E>(&mut self, tool_name: impl Into<String>, executor: E) -> Result<(), String>
    where
        E: LocalToolExecutor + 'static,
    {
        self.register_shared(tool_name, Arc::new(executor))
    }

    pub fn register_shared(
        &mut self,
        tool_name: impl Into<String>,
        executor: Arc<dyn LocalToolExecutor>,
    ) -> Result<(), String> {
        let tool_name = tool_name.into();
        let tool_name = tool_name.trim();
        if tool_name.is_empty() || tool_name.len() > 256 {
            return Err("Local tool name must be 1..=256 characters".to_string());
        }
        if self.executors.contains_key(tool_name) {
            return Err(format!("Local tool is registered twice: {tool_name}"));
        }
        self.executors.insert(tool_name.to_string(), executor);
        Ok(())
    }

    pub fn executor_for(&self, tool_name: &str) -> Option<Arc<dyn LocalToolExecutor>> {
        self.executors.get(tool_name).cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.executors.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolSchedulerTick {
    Idle,
    Committed(Box<LocalAgentToolCommitResult>),
}

#[derive(Debug, Error)]
pub enum LocalToolSchedulerError {
    #[error(transparent)]
    Runtime(#[from] LocalAgentRuntimeError),
    #[error("Local Agent runtime returned an unexpected tool result: {0}")]
    UnexpectedResult(&'static str),
}

#[derive(Clone)]
pub struct LocalToolScheduler {
    runtime: Arc<LocalAgentRuntime>,
    tools: LocalToolRegistry,
    owner_user_id: String,
    worker_id: String,
    lease_duration_ms: u64,
    include_tool_names: Option<Vec<String>>,
    exclude_tool_names: Vec<String>,
}

impl LocalToolScheduler {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        tools: LocalToolRegistry,
        owner_user_id: impl Into<String>,
        worker_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into();
        let owner_user_id = owner_user_id.trim();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Local tool owner user id must be 1..=256 characters".to_string());
        }
        let worker_id = worker_id.into();
        let worker_id = worker_id.trim();
        if worker_id.is_empty() || worker_id.len() > 256 {
            return Err("Local tool worker id must be 1..=256 characters".to_string());
        }
        if tools.is_empty() {
            return Err("Local tool scheduler requires at least one executor".to_string());
        }
        Ok(Self {
            runtime,
            tools,
            owner_user_id: owner_user_id.to_string(),
            worker_id: worker_id.to_string(),
            lease_duration_ms: 300_000,
            include_tool_names: None,
            exclude_tool_names: Vec::new(),
        })
    }

    pub fn with_lease_duration_ms(mut self, lease_duration_ms: u64) -> Result<Self, String> {
        if !(1_000..=300_000).contains(&lease_duration_ms) {
            return Err(
                "tool claim lease must be between 1000 and 300000 milliseconds".to_string(),
            );
        }
        self.lease_duration_ms = lease_duration_ms;
        Ok(self)
    }

    pub(crate) fn owner_user_id(&self) -> &str {
        &self.owner_user_id
    }

    pub fn with_tool_filter(
        mut self,
        include_tool_names: Option<Vec<String>>,
        exclude_tool_names: Vec<String>,
    ) -> Result<Self, String> {
        ClaimNextToolCommand {
            owner_user_id: self.owner_user_id.clone(),
            worker_id: self.worker_id.clone(),
            lease_duration_ms: self.lease_duration_ms,
            include_tool_names: include_tool_names.clone(),
            exclude_tool_names: exclude_tool_names.clone(),
        }
        .validate()?;
        self.include_tool_names = include_tool_names;
        self.exclude_tool_names = exclude_tool_names;
        Ok(self)
    }

    pub async fn run_once(&self) -> Result<ToolSchedulerTick, LocalToolSchedulerError> {
        let claimed = self
            .runtime
            .try_handle_ephemeral(envelope(
                "tool-scheduler-claim",
                HostCommand::ClaimNextTool(ClaimNextToolCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    worker_id: self.worker_id.clone(),
                    lease_duration_ms: self.lease_duration_ms,
                    include_tool_names: self.include_tool_names.clone(),
                    exclude_tool_names: self.exclude_tool_names.clone(),
                }),
            ))
            .await?;
        let claim = match claimed {
            HostResult::ToolClaim { claim: Some(claim) } => claim,
            HostResult::ToolClaim { claim: None } => return Ok(ToolSchedulerTick::Idle),
            _ => return Err(LocalToolSchedulerError::UnexpectedResult("claim")),
        };
        let outcome = match self.tools.executor_for(&claim.invocation.tool_name) {
            Some(executor) => match self.execute_with_heartbeat(executor, &claim).await? {
                None => return Ok(ToolSchedulerTick::Idle),
                Some(Ok(outcome)) => outcome,
                Some(Err(error)) if claim.invocation.side_effecting => {
                    LocalAgentToolOutcome::NeedsReview {
                        reason: "side-effecting tool returned an unknown result".to_string(),
                        detail: json!({"error": error}),
                    }
                }
                Some(Err(error)) => LocalAgentToolOutcome::Failed {
                    error,
                    detail: json!({"phase": "local_tool_execution"}),
                },
            },
            None => LocalAgentToolOutcome::NeedsReview {
                reason: "local tool executor is not registered".to_string(),
                detail: json!({"tool_name": claim.invocation.tool_name}),
            },
        };
        let committed = self
            .runtime
            .try_handle_ephemeral(envelope(
                "tool-scheduler-commit",
                HostCommand::CommitTool(CommitToolCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    invocation_id: claim.invocation.invocation_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.invocation.version,
                    outcome,
                }),
            ))
            .await?;
        match committed {
            HostResult::ToolCommit { result } => Ok(ToolSchedulerTick::Committed(result)),
            _ => Err(LocalToolSchedulerError::UnexpectedResult("commit")),
        }
    }

    async fn execute_with_heartbeat(
        &self,
        executor: Arc<dyn LocalToolExecutor>,
        claim: &chatos_local_agent_protocol::LocalAgentToolClaim,
    ) -> Result<Option<Result<LocalAgentToolOutcome, String>>, LocalAgentRuntimeError> {
        let execution = executor.execute_tool(&claim.invocation);
        tokio::pin!(execution);
        let heartbeat_interval = claim_heartbeat_interval(self.lease_duration_ms);
        loop {
            tokio::select! {
                outcome = &mut execution => return Ok(Some(outcome)),
                _ = tokio::time::sleep(heartbeat_interval) => {
                    let renewed = self.runtime.renew_tool_claim(
                        &self.owner_user_id,
                        &claim.invocation.invocation_id,
                        &claim.claim_token,
                        claim.invocation.version,
                        self.lease_duration_ms,
                    ).await?;
                    if !renewed {
                        return Ok(None);
                    }
                }
            }
        }
    }
}

fn claim_heartbeat_interval(lease_duration_ms: u64) -> Duration {
    Duration::from_millis((lease_duration_ms / 3).max(250))
}

fn envelope(prefix: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: format!("{prefix}-{}", Uuid::new_v4()),
        command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        ClaimNextRunCommand, CommitStepCommand, CreateRunCommand, LocalAgentRunStatus,
        LocalAgentStepOutcome, LocalAgentToolCall, LocalAgentToolStatus,
    };

    struct ReadFileTool;

    #[async_trait]
    impl LocalToolExecutor for ReadFileTool {
        async fn execute_tool(
            &self,
            invocation: &LocalAgentToolInvocationRecord,
        ) -> Result<LocalAgentToolOutcome, String> {
            Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({"path": invocation.arguments["path"], "content": "hello"}),
            })
        }
    }

    struct DelayedReadFileTool;

    #[async_trait]
    impl LocalToolExecutor for DelayedReadFileTool {
        async fn execute_tool(
            &self,
            invocation: &LocalAgentToolInvocationRecord,
        ) -> Result<LocalAgentToolOutcome, String> {
            tokio::time::sleep(Duration::from_millis(1_300)).await;
            Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({"path": invocation.arguments["path"], "content": "hello"}),
            })
        }
    }

    #[tokio::test]
    async fn tool_scheduler_executes_and_resumes_the_run() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                "create",
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-tool-scheduler".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: "conversation-1".to_string(),
                    profile_key: "main_chat".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": "hello"}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create");
        let claim = runtime
            .try_handle(envelope(
                "claim-run",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .expect("claim run");
        let claim = match claim {
            HostResult::Claim { claim: Some(claim) } => claim,
            result => panic!("unexpected result: {result:?}"),
        };
        runtime
            .try_handle(envelope(
                "commit-run",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".to_string(),
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: "batch-1".to_string(),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: "call-1".to_string(),
                            tool_name: "read_file".to_string(),
                            arguments: json!({"path": "README.md"}),
                            side_effecting: false,
                            requires_approval: false,
                        }],
                        checkpoint: json!({"response_id": "response-1"}),
                    },
                }),
            ))
            .await
            .expect("wait for tool");
        let mut tools = LocalToolRegistry::new();
        tools.register("read_file", ReadFileTool).expect("tool");
        let scheduler =
            LocalToolScheduler::new(runtime, tools, "user-1", "tool-worker").expect("scheduler");

        let ToolSchedulerTick::Committed(result) = scheduler.run_once().await.expect("tool step")
        else {
            panic!("expected committed tool")
        };
        assert_eq!(result.run.status, LocalAgentRunStatus::ContinuationReady);
        assert_eq!(
            result.invocation.result,
            Some(json!({
                "path": "README.md",
                "content": "hello"
            }))
        );
        assert_eq!(
            scheduler.run_once().await.expect("idle"),
            ToolSchedulerTick::Idle
        );
    }

    #[tokio::test]
    async fn tool_scheduler_renews_claim_during_long_execution() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                "create-heartbeat",
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-tool-heartbeat".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: "conversation-1".to_string(),
                    profile_key: "main_chat".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"message": "hello"}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create");
        let claim = runtime
            .try_handle(envelope(
                "claim-heartbeat-run",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .expect("claim run");
        let claim = match claim {
            HostResult::Claim { claim: Some(claim) } => claim,
            result => panic!("unexpected result: {result:?}"),
        };
        runtime
            .try_handle(envelope(
                "commit-heartbeat-run",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".to_string(),
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: "batch-heartbeat".to_string(),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: "call-heartbeat".to_string(),
                            tool_name: "read_file".to_string(),
                            arguments: json!({"path": "README.md"}),
                            side_effecting: false,
                            requires_approval: false,
                        }],
                        checkpoint: json!({"response_id": "response-heartbeat"}),
                    },
                }),
            ))
            .await
            .expect("wait for tool");
        let mut tools = LocalToolRegistry::new();
        tools
            .register("read_file", DelayedReadFileTool)
            .expect("tool");
        let scheduler = LocalToolScheduler::new(runtime, tools, "user-1", "tool-worker")
            .expect("scheduler")
            .with_lease_duration_ms(1_000)
            .expect("short lease");

        let ToolSchedulerTick::Committed(result) = scheduler.run_once().await.expect("long tool")
        else {
            panic!("expected committed tool")
        };
        assert_eq!(result.invocation.status, LocalAgentToolStatus::Succeeded);
        assert_eq!(result.run.status, LocalAgentRunStatus::ContinuationReady);
    }
}

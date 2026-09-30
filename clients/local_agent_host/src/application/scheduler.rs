// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! One-step scheduler for registered Local Agent business profiles.

use chatos_local_agent_protocol::{
    ClaimNextRunCommand, CommitStepCommand, GetRunCommand, HostCommand, HostRequestEnvelope,
    HostResult, LocalAgentRunRecord, LocalAgentStepOutcome, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{
    LocalAgentProfileRegistry, LocalAgentRuntime, LocalAgentRuntimeError,
};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum SchedulerTick {
    Idle,
    Committed(Box<LocalAgentRunRecord>),
}

#[derive(Debug, Error)]
pub enum LocalAgentSchedulerError {
    #[error(transparent)]
    Runtime(#[from] LocalAgentRuntimeError),
    #[error("Local Agent runtime returned an unexpected result: {0}")]
    UnexpectedResult(&'static str),
}

#[derive(Clone)]
pub struct LocalAgentScheduler {
    runtime: Arc<LocalAgentRuntime>,
    profiles: LocalAgentProfileRegistry,
    owner_user_id: String,
    worker_id: String,
    lease_duration_ms: u64,
}

impl LocalAgentScheduler {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        profiles: LocalAgentProfileRegistry,
        owner_user_id: impl Into<String>,
        worker_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into();
        let owner_user_id = owner_user_id.trim();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Local Agent owner user id must be 1..=256 characters".to_string());
        }
        let worker_id = worker_id.into();
        let worker_id = worker_id.trim();
        if worker_id.is_empty() || worker_id.len() > 256 {
            return Err("Local Agent worker id must be 1..=256 characters".to_string());
        }
        if profiles.is_empty() {
            return Err("Local Agent scheduler requires at least one profile".to_string());
        }
        Ok(Self {
            runtime,
            profiles,
            owner_user_id: owner_user_id.to_string(),
            worker_id: worker_id.to_string(),
            lease_duration_ms: 300_000,
        })
    }

    pub fn with_lease_duration_ms(mut self, lease_duration_ms: u64) -> Result<Self, String> {
        if !(1_000..=300_000).contains(&lease_duration_ms) {
            return Err("claim lease must be between 1000 and 300000 milliseconds".to_string());
        }
        self.lease_duration_ms = lease_duration_ms;
        Ok(self)
    }

    pub(crate) fn owner_user_id(&self) -> &str {
        &self.owner_user_id
    }

    /// Claims and executes at most one durable step. The caller owns wakeups
    /// and retry timers, so an idle Host does not create polling receipts.
    pub async fn run_once(&self) -> Result<SchedulerTick, LocalAgentSchedulerError> {
        self.runtime
            .start_next_task_run(&self.owner_user_id)
            .await?;
        let claim_result = self
            .runtime
            .try_handle(envelope(
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    worker_id: self.worker_id.clone(),
                    lease_duration_ms: self.lease_duration_ms,
                }),
                "scheduler-claim",
            ))
            .await?;
        let claim = match claim_result {
            HostResult::Claim { claim: Some(claim) } => claim,
            HostResult::Claim { claim: None } => return Ok(SchedulerTick::Idle),
            _ => return Err(LocalAgentSchedulerError::UnexpectedResult("claim")),
        };
        let outcome = match self.profiles.profile_for(&claim.run.profile_key) {
            Some(profile) => match profile.execute_step(&claim).await {
                Ok(outcome) => outcome,
                Err(error) => LocalAgentStepOutcome::NeedsReview {
                    reason: "Local Agent profile step failed".to_string(),
                    detail: serde_json::json!({"error": error}),
                },
            },
            None => LocalAgentStepOutcome::NeedsReview {
                reason: "Local Agent profile is not registered".to_string(),
                detail: serde_json::json!({"profile_key": claim.run.profile_key}),
            },
        };
        let run_id = claim.run.run_id.clone();
        let owner_user_id = claim.run.owner_user_id.clone();
        let claim_token = claim.claim_token.clone();
        let expected_version = claim.run.version;
        let committed = self
            .runtime
            .try_handle(envelope(
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: owner_user_id.clone(),
                    run_id: run_id.clone(),
                    claim_token: claim_token.clone(),
                    expected_version,
                    outcome,
                }),
                "scheduler-commit",
            ))
            .await;
        let committed = match committed {
            Ok(result) => result,
            Err(error) => {
                let current = self
                    .runtime
                    .try_handle(envelope(
                        HostCommand::GetRun(GetRunCommand {
                            owner_user_id,
                            run_id: run_id.clone(),
                        }),
                        "scheduler-reconcile",
                    ))
                    .await;
                match current {
                    Ok(HostResult::Run { run })
                        if run.version > expected_version
                            && run.claim_token.as_deref() != Some(claim_token.as_str()) =>
                    {
                        HostResult::Run { run }
                    }
                    _ => return Err(error.into()),
                }
            }
        };
        match committed {
            HostResult::Run { run } => Ok(SchedulerTick::Committed(Box::new(run))),
            _ => Err(LocalAgentSchedulerError::UnexpectedResult("commit")),
        }
    }
}

fn envelope(command: HostCommand, prefix: &str) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: format!("{prefix}-{}", Uuid::new_v4()),
        command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CreateConversationCommand, CreateRunCommand, CreateTaskGraphCommand, GetTaskGraphCommand,
        GuideConversationTurnCommand, LocalAgentRunClaim, LocalAgentRunStatus, LocalTaskSpec,
        LocalTaskStatus, StartConversationTurnCommand,
    };
    use chatos_local_agent_runtime::LocalAgentProfile;

    struct SuccessProfile;

    #[async_trait]
    impl LocalAgentProfile for SuccessProfile {
        async fn execute_step(
            &self,
            claim: &LocalAgentRunClaim,
        ) -> Result<LocalAgentStepOutcome, String> {
            Ok(LocalAgentStepOutcome::Succeed {
                output: serde_json::json!({"run_id": claim.run.run_id}),
            })
        }
    }

    struct GuidanceProfile {
        runtime: Arc<LocalAgentRuntime>,
    }

    #[async_trait]
    impl LocalAgentProfile for GuidanceProfile {
        async fn execute_step(
            &self,
            claim: &LocalAgentRunClaim,
        ) -> Result<LocalAgentStepOutcome, String> {
            if claim.run.continuation_input.is_some() {
                return Ok(LocalAgentStepOutcome::Succeed {
                    output: serde_json::json!({"answer": "guided"}),
                });
            }
            self.runtime
                .try_handle(envelope(
                    HostCommand::GuideConversationTurn(GuideConversationTurnCommand {
                        owner_user_id: "user-1".to_string(),
                        conversation_id: "conversation-guided-scheduler".to_string(),
                        expected_conversation_version: 2,
                        turn_id: "turn-guided-scheduler".to_string(),
                        expected_run_version: Some(claim.run.version),
                        message_id: "message-guided-scheduler-2".to_string(),
                        message: "change direction".to_string(),
                        message_metadata: serde_json::json!({}),
                        attachments: Vec::new(),
                    }),
                    "guide-active-step",
                ))
                .await
                .map_err(|error| error.to_string())?;
            Ok(LocalAgentStepOutcome::Succeed {
                output: serde_json::json!({"answer": "stale"}),
            })
        }
    }

    #[tokio::test]
    async fn scheduler_executes_one_registered_profile_step() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-scheduler".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "conversation".to_string(),
                    owner_entity_id: "conversation-1".to_string(),
                    profile_key: "test_success".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: serde_json::json!({"message": "hello"}),
                    max_iterations: 4,
                }),
                "create",
            ))
            .await
            .expect("create run");
        let mut profiles = LocalAgentProfileRegistry::new();
        profiles
            .register("test_success", SuccessProfile)
            .expect("profile");
        let scheduler =
            LocalAgentScheduler::new(runtime, profiles, "user-1", "worker-1").expect("scheduler");

        let tick = scheduler.run_once().await.expect("run once");
        let SchedulerTick::Committed(run) = tick else {
            panic!("expected committed run")
        };
        assert_eq!(run.status, LocalAgentRunStatus::Succeeded);
        assert_eq!(run.version, 3);
        assert_eq!(
            scheduler.run_once().await.expect("idle"),
            SchedulerTick::Idle
        );
    }

    #[tokio::test]
    async fn scheduler_reconciles_a_model_claim_superseded_by_guidance() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: "conversation-guided-scheduler".to_string(),
                    owner_user_id: "user-1".to_string(),
                    title: "Guided scheduler".to_string(),
                    resource: None,
                }),
                "create-guided-conversation",
            ))
            .await
            .expect("create conversation");
        runtime
            .try_handle(envelope(
                HostCommand::StartConversationTurn(StartConversationTurnCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-guided-scheduler".to_string(),
                    expected_conversation_version: 1,
                    turn_id: "turn-guided-scheduler".to_string(),
                    message_id: "message-guided-scheduler-1".to_string(),
                    run_id: "run-guided-scheduler".to_string(),
                    message: "start".to_string(),
                    message_metadata: serde_json::json!({}),
                    attachments: Vec::new(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    max_iterations: 4,
                }),
                "start-guided-turn",
            ))
            .await
            .expect("start Turn");
        let mut profiles = LocalAgentProfileRegistry::new();
        profiles
            .register(
                "main_chat",
                GuidanceProfile {
                    runtime: Arc::clone(&runtime),
                },
            )
            .expect("profile");
        let scheduler =
            LocalAgentScheduler::new(runtime, profiles, "user-1", "worker-1").expect("scheduler");

        let SchedulerTick::Committed(interrupted) = scheduler.run_once().await.expect("interrupt")
        else {
            panic!("expected interrupted claim reconciliation")
        };
        assert_eq!(interrupted.status, LocalAgentRunStatus::ContinuationReady);
        let SchedulerTick::Committed(completed) = scheduler.run_once().await.expect("complete")
        else {
            panic!("expected guided completion")
        };
        assert_eq!(completed.status, LocalAgentRunStatus::Succeeded);
        assert_eq!(
            completed.terminal_outcome,
            Some(serde_json::json!({"answer": "guided"}))
        );
    }

    #[tokio::test]
    async fn scheduler_materializes_and_completes_a_ready_task() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                HostCommand::CreateTaskGraph(CreateTaskGraphCommand {
                    graph_id: "graph-scheduler".to_string(),
                    owner_user_id: "user-1".to_string(),
                    source_entity_type: "conversation".to_string(),
                    source_entity_id: "conversation-1".to_string(),
                    tasks: vec![LocalTaskSpec {
                        task_id: "task-scheduler".to_string(),
                        title: "Scheduled task".to_string(),
                        profile_key: "test_success".to_string(),
                        model_config_ref: "model-1".to_string(),
                        model_config_revision: "revision-1".to_string(),
                        capability_policy_revision: "policy-1".to_string(),
                        input: serde_json::json!({"message": "hello"}),
                        max_iterations: 4,
                    }],
                    dependencies: Vec::new(),
                }),
                "create-task-graph",
            ))
            .await
            .expect("create graph");
        let mut profiles = LocalAgentProfileRegistry::new();
        profiles
            .register("test_success", SuccessProfile)
            .expect("profile");
        let scheduler =
            LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "worker-1")
                .expect("scheduler");

        assert!(matches!(
            scheduler.run_once().await.expect("run task"),
            SchedulerTick::Committed(_)
        ));
        let graph = runtime
            .try_handle(envelope(
                HostCommand::GetTaskGraph(GetTaskGraphCommand {
                    owner_user_id: "user-1".to_string(),
                    graph_id: "graph-scheduler".to_string(),
                }),
                "get-task-graph",
            ))
            .await
            .expect("get graph");
        let HostResult::TaskGraph { graph } = graph else {
            panic!("expected task graph")
        };
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Succeeded);
        assert!(graph.tasks[0].active_run_id.is_none());
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata,
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{LocalAgentHost, LocalAgentHostPolicy, LocalAgentProfileRegistry};
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRun,
    LocalAgentRunStatus, ModelGatewayRequest, ModelGatewayTokenCount, ModelProtocol,
    ModelRuntimeDescriptor, ModelStepResult,
};
use chatos_local_agent_runtime::{
    LocalAgentProfile, LocalAgentProfileStep, ModelGatewayCallbacks, ModelGatewayClient,
    ModelGatewayClientError, ModelGatewayOutput, SchedulerTickResult, StepEvidence,
};
use chrono::Utc;
use tokio_util::sync::CancellationToken;

struct Profile;

#[async_trait]
impl LocalAgentProfile for Profile {
    fn profile_key(&self) -> &'static str {
        "main_chat"
    }

    async fn prepare_model_step(
        &self,
        _run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        unreachable!("not executed by lifecycle test")
    }

    async fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        _output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        unreachable!("not executed by lifecycle test")
    }
}

struct Gateway;

#[async_trait]
impl ModelGatewayClient for Gateway {
    async fn descriptor(
        &self,
        _access_token: &str,
        _model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        unreachable!("not executed by lifecycle test")
    }

    async fn stream(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        _request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        unreachable!("not executed by lifecycle test")
    }

    async fn count_input_tokens(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        _request: &ModelGatewayRequest,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayTokenCount, ModelGatewayClientError> {
        unreachable!("not executed by lifecycle test")
    }
}

struct Seed {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for Seed {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let scope = scope();
        let metadata = |id: &str| RecordMetadata {
            id: id.to_string(),
            scope: scope.clone(),
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: self.now,
            updated_at: self.now,
        };
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: metadata("run-1"),
                    run: run(self.now),
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_events()
            .put(PutRecord {
                record: AgentEventStateRecord {
                    metadata: metadata("event-1"),
                    event: LocalAgentEvent {
                        event_id: "event-1".to_string(),
                        run_id: "run-1".to_string(),
                        event_type: LocalAgentEventType::RunStarted,
                        expected_version: 1,
                        available_at: self.now,
                        status: LocalAgentEventStatus::Pending,
                        attempt_count: 0,
                        claimed_by_device_id: None,
                        claim_token: None,
                        claim_until: None,
                        causation_id: "turn-1".to_string(),
                        correlation_id: "thread-1-1".to_string(),
                        bounded_payload: serde_json::Value::Null,
                        last_error: None,
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn host_recovers_claims_commits_and_schedules_the_next_event() {
    let now = Utc::now();
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:key").unwrap(),
            },
            &StorageEncryptionKey::new([7; 32]),
        )
        .await
        .unwrap(),
    );
    storage.transaction(&mut Seed { now }).await.unwrap();
    let profiles =
        LocalAgentProfileRegistry::new([Arc::new(Profile) as Arc<dyn LocalAgentProfile>]).unwrap();
    let (host, report) = LocalAgentHost::start(
        storage,
        Arc::new(Gateway),
        profiles,
        scope(),
        "device-1",
        LocalAgentHostPolicy::default(),
        now,
    )
    .await
    .unwrap();

    assert_eq!(report.active_run_count, 1);
    assert_eq!(report.ready_event_count, 1);
    assert!(report.recovery_issues.is_empty());
    assert_eq!(
        host.profiles().require("main_chat").unwrap().profile_key(),
        "main_chat"
    );

    let SchedulerTickResult::Claimed(started) = host.claim_next("claim-1", now).await.unwrap()
    else {
        panic!("run_started was not claimed");
    };
    host.commit_claimed(&started, StepEvidence::None, now)
        .await
        .unwrap();

    let SchedulerTickResult::Claimed(next) = host.claim_next("claim-2", now).await.unwrap() else {
        panic!("model step request was not scheduled");
    };
    assert_eq!(
        next.event.event_type,
        LocalAgentEventType::ModelStepRequested
    );
    assert_eq!(next.event.expected_version, 2);
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn run(now: chrono::DateTime<Utc>) -> LocalAgentRun {
    let descriptor = ModelRuntimeDescriptor {
        model_config_id: "model-1".to_string(),
        revision: 1,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    };
    LocalAgentRun {
        run_id: "run-1".to_string(),
        profile_key: "main_chat".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "thread-1".to_string(),
        project_id: None,
        status: LocalAgentRunStatus::Queued,
        version: 1,
        step_seq: 0,
        iteration: 0,
        retry_count: 0,
        model_config_id: "model-1".to_string(),
        model_config_revision: 1,
        model_runtime_snapshot: descriptor,
        context_strategy: ContextStrategy::ProviderNative,
        prompt_revision: "prompt-1".to_string(),
        capability_snapshot_ref: "capabilities-1".to_string(),
        pending_batch_id: None,
        pending_interaction: None,
        terminal_outcome: None,
        deadline_at: None,
        created_at: now,
        updated_at: now,
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::VecDeque, sync::Arc};

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentRun, LocalAgentRunStatus, ModelGatewayRequest, ModelGatewayTerminal,
    ModelGatewayTerminalSource, ModelGatewayTerminalStatus, ModelGatewayTokenCount, ModelProtocol,
    ModelRuntimeDescriptor, ModelStepResult,
};
use chatos_local_agent_runtime::{
    prepare_model_step_persistence, ExecutedModelStep, LocalAgentProfile, LocalAgentProfileStep,
    MemoryEngineContextAdapter, MemoryEngineContextApi, MemoryEngineContextScope,
    ModelGatewayCallbacks, ModelGatewayClient, ModelGatewayClientError, ModelGatewayOutput,
    ModelStepContext, ModelStepExecutorError, ModelStepPersistenceError,
    ProviderNativeContextWindow, SingleModelStepExecutor,
};
use chatos_memory_client::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextRequest, ComposeContextResponse,
    RunThreadActiveSummaryResponse,
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

struct TestProfile;

#[async_trait]
impl LocalAgentProfile for TestProfile {
    fn profile_key(&self) -> &'static str {
        "test_profile"
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        Ok(LocalAgentProfileStep {
            model_input_items: vec![json!({
                "type": "message",
                "role": "user",
                "content": "current step"
            })],
            tools: Vec::new(),
            instructions: Some("Complete one step.".to_string()),
            maximum_output_tokens: 32_000,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: (run.context_strategy == ContextStrategy::ProviderNative)
                .then_some(200_000),
            memory_engine_active_threshold: (run.context_strategy == ContextStrategy::MemoryEngine)
                .then_some(200_000),
            maximum_summary_attempts: if run.context_strategy == ContextStrategy::MemoryEngine {
                2
            } else {
                0
            },
        })
    }

    async fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        Ok(ModelStepResult::Final(json!({"text": output.content})))
    }
}

struct MockGateway {
    counts: Mutex<VecDeque<u64>>,
    requests: Mutex<Vec<ModelGatewayRequest>>,
    output: ModelGatewayOutput,
}

#[async_trait]
impl ModelGatewayClient for MockGateway {
    async fn descriptor(
        &self,
        _access_token: &str,
        _model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        unreachable!("not used")
    }

    async fn stream(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        self.requests.lock().await.push(request);
        Ok(self.output.clone())
    }

    async fn count_input_tokens(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        request: &ModelGatewayRequest,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayTokenCount, ModelGatewayClientError> {
        let input_tokens = self
            .counts
            .lock()
            .await
            .pop_front()
            .expect("configured token count");
        Ok(ModelGatewayTokenCount {
            request_id: request.request_id.clone(),
            model_config_id: request.model_config_id.clone(),
            model_config_revision: request.model_config_revision,
            input_tokens,
        })
    }
}

#[tokio::test]
async fn provider_context_commits_only_after_a_completed_terminal() {
    let output = completed_output(vec![
        json!({
            "type": "compaction",
            "id": "compaction-1",
            "encrypted_content": "opaque"
        }),
        json!({"type": "message", "id": "message-1", "content": []}),
    ]);
    let gateway = Arc::new(MockGateway {
        counts: Mutex::new(VecDeque::from([210_000])),
        requests: Mutex::new(Vec::new()),
        output,
    });
    let executor = SingleModelStepExecutor::new(gateway.clone());

    let executed = executor
        .execute(
            "user-token",
            &run(ContextStrategy::ProviderNative),
            "request-1",
            &TestProfile,
            ModelStepContext::ProviderNative(ProviderNativeContextWindow::empty(1).unwrap()),
            ModelGatewayCallbacks::default(),
            CancellationToken::new(),
        )
        .await
        .expect("executed step");

    assert!(matches!(executed.result, ModelStepResult::Final(_)));
    let commit = executed
        .provider_context_commit
        .expect("provider context commit");
    assert_eq!(commit.newest_compaction_id.as_deref(), Some("compaction-1"));
    assert_eq!(commit.retained_items.len(), 2);
    assert_eq!(gateway.requests.lock().await.len(), 1);
}

#[tokio::test]
async fn memory_context_summarizes_recomposes_and_recounts_before_streaming() {
    let memory = Arc::new(MockMemoryApi {
        compose_responses: Mutex::new(VecDeque::from([
            compose_response("before summary"),
            compose_response("after summary"),
        ])),
        summary_runs: Mutex::new(0),
    });
    let adapter = MemoryEngineContextAdapter::new(memory.clone(), "source-1").unwrap();
    let gateway = Arc::new(MockGateway {
        counts: Mutex::new(VecDeque::from([210_000, 120_000])),
        requests: Mutex::new(Vec::new()),
        output: completed_output(Vec::new()),
    });
    let executor = SingleModelStepExecutor::new(gateway.clone());

    let executed = executor
        .execute(
            "user-token",
            &run(ContextStrategy::MemoryEngine),
            "request-2",
            &TestProfile,
            ModelStepContext::MemoryEngine {
                adapter,
                scope: MemoryEngineContextScope::thread("tenant-1", "source-1", "thread-1")
                    .unwrap(),
            },
            ModelGatewayCallbacks::default(),
            CancellationToken::new(),
        )
        .await
        .expect("executed step");

    assert_eq!(executed.token_assessments.len(), 2);
    assert_eq!(*memory.summary_runs.lock().await, 1);
    let requests = gateway.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].input.to_string().contains("after summary"));
    assert!(!requests[0].input.to_string().contains("before summary"));
}

#[tokio::test]
async fn summary_without_token_improvement_blocks_model_io() {
    let memory = Arc::new(MockMemoryApi {
        compose_responses: Mutex::new(VecDeque::from([
            compose_response("before"),
            compose_response("unchanged"),
        ])),
        summary_runs: Mutex::new(0),
    });
    let adapter = MemoryEngineContextAdapter::new(memory, "source-1").unwrap();
    let gateway = Arc::new(MockGateway {
        counts: Mutex::new(VecDeque::from([210_000, 210_000])),
        requests: Mutex::new(Vec::new()),
        output: completed_output(Vec::new()),
    });
    let executor = SingleModelStepExecutor::new(gateway.clone());

    let error = executor
        .execute(
            "user-token",
            &run(ContextStrategy::MemoryEngine),
            "request-3",
            &TestProfile,
            ModelStepContext::MemoryEngine {
                adapter,
                scope: MemoryEngineContextScope::thread("tenant-1", "source-1", "thread-1")
                    .unwrap(),
            },
            ModelGatewayCallbacks::default(),
            CancellationToken::new(),
        )
        .await
        .expect_err("no improvement");

    assert_eq!(
        error,
        ModelStepExecutorError::ContextReductionNoImprovement {
            before: 210_000,
            after: 210_000,
        }
    );
    assert!(gateway.requests.lock().await.is_empty());
}

#[test]
fn executed_model_output_gets_stable_completion_and_message_identities() {
    let now = Utc::now();
    let executed = ExecutedModelStep {
        result: ModelStepResult::Final(json!({"text": "done"})),
        output: Some(completed_output(Vec::new())),
        token_assessments: Vec::new(),
        provider_context_commit: None,
    };
    let first = prepare_model_step_persistence(
        &run(ContextStrategy::ProviderNative),
        "model-request-1",
        "turn-1",
        executed.clone(),
        None,
        now,
    )
    .unwrap();
    let repeated = prepare_model_step_persistence(
        &run(ContextStrategy::ProviderNative),
        "model-request-1",
        "turn-1",
        executed,
        None,
        now + Duration::seconds(1),
    )
    .unwrap();
    assert_eq!(first, repeated);
    assert!(first.completion.pending_batch_id.is_none());
    let message = first.assistant_message.unwrap();
    assert!(message.record_id.starts_with("assistant-model-output:"));
    assert_eq!(message.turn_id, "turn-1");
    assert_eq!(message.content.as_deref(), Some("done"));
    assert_eq!(message.response_id.as_deref(), Some("response-1"));
}

#[test]
fn tool_and_retry_metadata_are_derived_once_by_the_runtime() {
    let now = Utc::now();
    let mut tool_output = completed_output(Vec::new());
    tool_output.content.clear();
    let tool = prepare_model_step_persistence(
        &run(ContextStrategy::ProviderNative),
        "model-request-2",
        "turn-2",
        ExecutedModelStep {
            result: ModelStepResult::ToolCommand(json!({"calls": [{"call_id": "call-1"}]})),
            output: Some(tool_output),
            token_assessments: Vec::new(),
            provider_context_commit: None,
        },
        None,
        now,
    )
    .unwrap();
    assert!(tool.completion.pending_batch_id.is_some());
    assert!(tool.assistant_message.is_none());

    let retry_result = ExecutedModelStep {
        result: ModelStepResult::Retry(json!({"reason": "temporary"})),
        output: Some(completed_output(Vec::new())),
        token_assessments: Vec::new(),
        provider_context_commit: None,
    };
    assert_eq!(
        prepare_model_step_persistence(
            &run(ContextStrategy::ProviderNative),
            "model-request-3",
            "turn-3",
            retry_result,
            None,
            now,
        )
        .unwrap_err(),
        ModelStepPersistenceError::InvalidRetryDeadline
    );
}

struct MockMemoryApi {
    compose_responses: Mutex<VecDeque<ComposeContextResponse>>,
    summary_runs: Mutex<usize>,
}

#[async_trait]
impl MemoryEngineContextApi for MockMemoryApi {
    async fn compose_context(
        &self,
        _request: &ComposeContextRequest,
    ) -> Result<ComposeContextResponse, String> {
        self.compose_responses
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| "missing compose response".to_string())
    }

    async fn run_active_summary(
        &self,
        _thread_id: &str,
        _tenant_id: &str,
        _trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        *self.summary_runs.lock().await += 1;
        Ok(summary_status(true))
    }

    async fn get_active_summary_status(
        &self,
        _thread_id: &str,
        _tenant_id: &str,
        _job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        Ok(summary_status(false))
    }
}

fn run(strategy: ContextStrategy) -> LocalAgentRun {
    let now = Utc::now();
    LocalAgentRun {
        run_id: "run-1".to_string(),
        profile_key: "test_profile".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "thread-1".to_string(),
        project_id: None,
        status: LocalAgentRunStatus::ModelRunning,
        version: 2,
        step_seq: 1,
        iteration: 0,
        retry_count: 0,
        model_config_id: "model-1".to_string(),
        model_config_revision: 7,
        model_runtime_snapshot: ModelRuntimeDescriptor {
            model_config_id: "model-1".to_string(),
            revision: 7,
            provider: "configured-provider".to_string(),
            model: "configured-model".to_string(),
            protocol: ModelProtocol::Responses,
            context_window_tokens: 400_000,
            maximum_output_tokens: 32_000,
            context_strategy: strategy,
            supports_streaming: true,
            supports_native_compaction: strategy == ContextStrategy::ProviderNative,
            supports_input_token_count: true,
        },
        context_strategy: strategy,
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

fn completed_output(output_items: Vec<Value>) -> ModelGatewayOutput {
    ModelGatewayOutput {
        content: "done".to_string(),
        reasoning: String::new(),
        output_items: output_items.clone(),
        terminal: ModelGatewayTerminal {
            status: ModelGatewayTerminalStatus::Completed,
            source: ModelGatewayTerminalSource::Provider,
            response_id: Some("response-1".to_string()),
            provider_request_id: Some("provider-request-1".to_string()),
            terminal_event: "response.completed".to_string(),
            provider_http_status: Some(200),
            usage: Some(json!({"input_tokens": 100, "output_tokens": 10})),
            output_items,
            incomplete_details: None,
            provider_error: None,
        },
    }
}

fn compose_response(summary: &str) -> ComposeContextResponse {
    ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: vec![ComposeContextBlock {
            block_type: "thread_summary".to_string(),
            text: summary.to_string(),
        }],
        recent_records: Vec::new(),
        meta: ComposeContextMeta {
            summary_count: 1,
            recent_record_count: 0,
        },
    }
}

fn summary_status(completed: bool) -> RunThreadActiveSummaryResponse {
    RunThreadActiveSummaryResponse {
        thread_id: "thread-1".to_string(),
        accepted: completed,
        running: false,
        completed,
        failed: false,
        job_run_id: None,
        generated: completed,
        summary_id: completed.then(|| "summary-1".to_string()),
        source_record_count: 1,
        pending_before_count: Some(1),
        pending_after_count: Some(0),
        compacted: completed,
        error_message: None,
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentRun, LocalAgentRunStatus, ModelGatewayParameters,
    ModelGatewayRequest, ModelGatewayTerminalStatus, ModelStepCompletion, ModelStepResult,
    MAX_BOUNDED_JSON_BYTES, MAX_MODEL_GATEWAY_JSON_BYTES,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{
    digest::stable_digest_id, guard_model_input, CompletedAssistantMessage,
    MemoryEngineContextAdapter, MemoryEngineContextError, MemoryEngineContextScope,
    ModelGatewayCallbacks, ModelGatewayClient, ModelGatewayClientError, ModelGatewayOutput,
    ModelInputTokenAction, ModelInputTokenAssessment, ModelInputTokenGuardError,
    ProviderNativeContextCommit, ProviderNativeContextError, ProviderNativeContextWindow,
};

const MAX_MEMORY_SUMMARY_ATTEMPTS: u8 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct LocalAgentProfileStep {
    /// Strategy-specific items not already present in its durable context.
    /// Profiles must not re-add Memory Engine records returned by compose.
    pub model_input_items: Vec<Value>,
    pub tools: Vec<Value>,
    pub instructions: Option<String>,
    pub maximum_output_tokens: u32,
    pub reasoning_effort: Option<String>,
    pub temperature: Option<f64>,
    pub native_compaction_threshold: Option<u64>,
    pub memory_engine_active_threshold: Option<u64>,
    pub maximum_summary_attempts: u8,
}

impl LocalAgentProfileStep {
    fn validate(&self, run: &LocalAgentRun) -> Result<(), ModelStepExecutorError> {
        if self.maximum_output_tokens == 0
            || self.maximum_output_tokens > run.model_runtime_snapshot.maximum_output_tokens
        {
            return invalid_profile("profile output budget exceeds the frozen model descriptor");
        }
        validate_json_items("model input", self.model_input_items.as_slice())?;
        validate_json_items("tools", self.tools.as_slice())?;
        if self.instructions.as_deref().is_some_and(str::is_empty) {
            return invalid_profile("profile instructions cannot be empty when present");
        }
        match run.context_strategy {
            ContextStrategy::ProviderNative => {
                if self.native_compaction_threshold.is_none()
                    || self.memory_engine_active_threshold.is_some()
                    || self.maximum_summary_attempts != 0
                {
                    return invalid_profile(
                        "provider-native profile step has incompatible context settings",
                    );
                }
            }
            ContextStrategy::MemoryEngine => {
                if self.native_compaction_threshold.is_some()
                    || self.memory_engine_active_threshold.is_none()
                    || self.maximum_summary_attempts == 0
                    || self.maximum_summary_attempts > MAX_MEMORY_SUMMARY_ATTEMPTS
                {
                    return invalid_profile(
                        "Memory Engine profile step has incompatible context settings",
                    );
                }
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait LocalAgentProfile: Send + Sync {
    fn profile_key(&self) -> &'static str;

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String>;

    async fn interpret_completed_output(
        &self,
        run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String>;
}

pub enum ModelStepContext {
    ProviderNative(ProviderNativeContextWindow),
    MemoryEngine {
        adapter: MemoryEngineContextAdapter,
        scope: MemoryEngineContextScope,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutedModelStep {
    pub result: ModelStepResult,
    pub output: Option<ModelGatewayOutput>,
    pub token_assessments: Vec<ModelInputTokenAssessment>,
    pub provider_context_commit: Option<ProviderNativeContextCommit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedModelStepPersistence {
    pub completion: ModelStepCompletion,
    pub assistant_message: Option<CompletedAssistantMessage>,
    pub provider_context_commit: Option<ProviderNativeContextCommit>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelStepPersistenceError {
    #[error("model request and turn identifiers must not be empty")]
    InvalidIdentity,
    #[error("retry model result requires exactly one future retry deadline")]
    InvalidRetryDeadline,
    #[error("model result could not be represented as semantic JSON")]
    InvalidSemanticResult,
}

/// Converts one executed model request into stable persistence artifacts. The
/// caller still seals provider-native items before committing them, but batch
/// and assistant record identities are decided once here rather than by each
/// business Profile or native client.
pub fn prepare_model_step_persistence(
    run: &LocalAgentRun,
    request_event_id: &str,
    turn_id: &str,
    executed: ExecutedModelStep,
    retry_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<PreparedModelStepPersistence, ModelStepPersistenceError> {
    if request_event_id.trim().is_empty() || turn_id.trim().is_empty() {
        return Err(ModelStepPersistenceError::InvalidIdentity);
    }
    let retry_at = match (&executed.result, retry_at) {
        (ModelStepResult::Retry(_), Some(deadline)) if deadline > now => Some(deadline),
        (ModelStepResult::Retry(_), _) | (_, Some(_)) => {
            return Err(ModelStepPersistenceError::InvalidRetryDeadline);
        }
        (_, None) => None,
    };
    let pending_batch_id = matches!(&executed.result, ModelStepResult::ToolCommand(_)).then(|| {
        stable_digest_id(
            "tool-batch",
            &[run.owner_user_id.as_str(), request_event_id],
        )
    });
    let assistant_message = match executed
        .output
        .as_ref()
        .filter(|output| output.terminal.status == ModelGatewayTerminalStatus::Completed)
    {
        Some(output) => {
            let content = (!output.content.trim().is_empty()).then(|| output.content.clone());
            let reasoning = (!output.reasoning.trim().is_empty()).then(|| output.reasoning.clone());
            let structured_payload = if matches!(&executed.result, ModelStepResult::ToolCommand(_))
            {
                None
            } else {
                Some(
                    serde_json::to_value(&executed.result)
                        .map_err(|_| ModelStepPersistenceError::InvalidSemanticResult)?,
                )
            };
            (content.is_some() || reasoning.is_some() || structured_payload.is_some()).then(|| {
                CompletedAssistantMessage {
                    record_id: stable_digest_id(
                        "assistant-model-output",
                        &[run.owner_user_id.as_str(), request_event_id],
                    ),
                    turn_id: turn_id.to_string(),
                    content,
                    reasoning,
                    structured_payload,
                    response_id: output.terminal.response_id.clone(),
                    message_source: format!("{}_model", run.profile_key),
                }
            })
        }
        None => None,
    };
    Ok(PreparedModelStepPersistence {
        completion: ModelStepCompletion {
            result: executed.result,
            pending_batch_id,
            retry_at,
        },
        assistant_message,
        provider_context_commit: executed.provider_context_commit,
    })
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelStepExecutorError {
    #[error("invalid local Agent run: {0}")]
    InvalidRun(String),
    #[error("profile {actual} cannot execute run profile {expected}")]
    ProfileMismatch { expected: String, actual: String },
    #[error("local Agent profile failed: {0}")]
    Profile(String),
    #[error("model step profile contract is invalid: {0}")]
    InvalidProfile(String),
    #[error("model step context strategy does not match the frozen run")]
    ContextStrategyMismatch,
    #[error(transparent)]
    ProviderContext(#[from] ProviderNativeContextError),
    #[error(transparent)]
    MemoryContext(#[from] MemoryEngineContextError),
    #[error(transparent)]
    TokenGuard(#[from] ModelInputTokenGuardError),
    #[error(transparent)]
    Gateway(#[from] ModelGatewayClientError),
    #[error("Memory Engine summary did not reduce model input tokens ({before} -> {after})")]
    ContextReductionNoImprovement { before: u64, after: u64 },
    #[error("Memory Engine input still requires reduction after {attempts} summary attempts")]
    ContextReductionAttemptsExhausted { attempts: u8 },
    #[error("model step result exceeds the durable event payload limit")]
    ResultTooLarge,
}

#[derive(Clone)]
pub struct SingleModelStepExecutor {
    gateway: Arc<dyn ModelGatewayClient>,
}

impl SingleModelStepExecutor {
    pub fn new(gateway: Arc<dyn ModelGatewayClient>) -> Self {
        Self { gateway }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        access_token: &str,
        run: &LocalAgentRun,
        request_id: impl Into<String>,
        profile: &dyn LocalAgentProfile,
        context: ModelStepContext,
        callbacks: ModelGatewayCallbacks,
        cancellation: CancellationToken,
    ) -> Result<ExecutedModelStep, ModelStepExecutorError> {
        run.validate()
            .map_err(|error| ModelStepExecutorError::InvalidRun(error.to_string()))?;
        if run.status != LocalAgentRunStatus::ModelRunning {
            return Err(ModelStepExecutorError::InvalidRun(
                "a model step can execute only while the run is model_running".to_string(),
            ));
        }
        if profile.profile_key() != run.profile_key {
            return Err(ModelStepExecutorError::ProfileMismatch {
                expected: run.profile_key.clone(),
                actual: profile.profile_key().to_string(),
            });
        }
        let step = profile
            .prepare_model_step(run)
            .await
            .map_err(ModelStepExecutorError::Profile)?;
        step.validate(run)?;
        let request_id = request_id.into();

        match (run.context_strategy, context) {
            (ContextStrategy::ProviderNative, ModelStepContext::ProviderNative(mut window)) => {
                let input = window.request_input(step.model_input_items.as_slice())?;
                let request = build_request(run, request_id, Value::Array(input), &step);
                let assessment = match guard_model_input(
                    self.gateway.as_ref(),
                    access_token,
                    &run.model_runtime_snapshot,
                    &request,
                    None,
                    cancellation.clone(),
                )
                .await
                {
                    Err(ModelInputTokenGuardError::ExactCount(
                        ModelGatewayClientError::Cancelled,
                    )) => return Ok(cancelled_execution(Vec::new())),
                    result => result?,
                };
                debug_assert_ne!(
                    assessment.action,
                    ModelInputTokenAction::RequireMemoryEngineSummary
                );
                let output = self
                    .gateway
                    .stream(
                        access_token,
                        &run.model_runtime_snapshot,
                        request,
                        callbacks,
                        cancellation,
                    )
                    .await;
                let output = match output {
                    Err(ModelGatewayClientError::Cancelled) => {
                        return Ok(cancelled_execution(vec![assessment]));
                    }
                    result => result?,
                };
                let (result, provider_context_commit) = match output.terminal.status {
                    ModelGatewayTerminalStatus::Completed => {
                        let commit = window.commit_response(
                            step.model_input_items.as_slice(),
                            output.terminal.output_items.as_slice(),
                        )?;
                        (
                            profile
                                .interpret_completed_output(run, &output)
                                .await
                                .map_err(ModelStepExecutorError::Profile)?,
                            Some(commit),
                        )
                    }
                    ModelGatewayTerminalStatus::Incomplete | ModelGatewayTerminalStatus::Failed => {
                        (terminal_failure_result(&output), None)
                    }
                };
                validate_result_size(&result)?;
                Ok(ExecutedModelStep {
                    result,
                    output: Some(output),
                    token_assessments: vec![assessment],
                    provider_context_commit,
                })
            }
            (ContextStrategy::MemoryEngine, ModelStepContext::MemoryEngine { adapter, scope }) => {
                let mut prepared = match adapter.prepare_model_input(&scope, &cancellation).await {
                    Err(MemoryEngineContextError::Cancelled) => {
                        return Ok(cancelled_execution(Vec::new()));
                    }
                    result => result?,
                };
                let mut assessments = Vec::new();
                let mut attempts = 0u8;
                let mut previous_tokens_before_summary = None;
                loop {
                    let input = merge_input_items(
                        prepared.input_items.as_slice(),
                        step.model_input_items.as_slice(),
                    );
                    let request =
                        build_request(run, request_id.clone(), Value::Array(input), &step);
                    let assessment = match guard_model_input(
                        self.gateway.as_ref(),
                        access_token,
                        &run.model_runtime_snapshot,
                        &request,
                        step.memory_engine_active_threshold,
                        cancellation.clone(),
                    )
                    .await
                    {
                        Err(ModelInputTokenGuardError::ExactCount(
                            ModelGatewayClientError::Cancelled,
                        )) => return Ok(cancelled_execution(assessments)),
                        result => result?,
                    };
                    if let Some(before) = previous_tokens_before_summary.take() {
                        if assessment.input_tokens >= before {
                            return Err(ModelStepExecutorError::ContextReductionNoImprovement {
                                before,
                                after: assessment.input_tokens,
                            });
                        }
                    }
                    assessments.push(assessment);
                    if assessment.action != ModelInputTokenAction::RequireMemoryEngineSummary {
                        let output = self
                            .gateway
                            .stream(
                                access_token,
                                &run.model_runtime_snapshot,
                                request,
                                callbacks,
                                cancellation,
                            )
                            .await;
                        let output = match output {
                            Err(ModelGatewayClientError::Cancelled) => {
                                return Ok(cancelled_execution(assessments));
                            }
                            result => result?,
                        };
                        let result = match output.terminal.status {
                            ModelGatewayTerminalStatus::Completed => profile
                                .interpret_completed_output(run, &output)
                                .await
                                .map_err(ModelStepExecutorError::Profile)?,
                            ModelGatewayTerminalStatus::Incomplete
                            | ModelGatewayTerminalStatus::Failed => {
                                terminal_failure_result(&output)
                            }
                        };
                        validate_result_size(&result)?;
                        return Ok(ExecutedModelStep {
                            result,
                            output: Some(output),
                            token_assessments: assessments,
                            provider_context_commit: None,
                        });
                    }
                    if attempts >= step.maximum_summary_attempts {
                        return Err(ModelStepExecutorError::ContextReductionAttemptsExhausted {
                            attempts,
                        });
                    }
                    attempts = attempts.saturating_add(1);
                    previous_tokens_before_summary = Some(assessment.input_tokens);
                    prepared = match adapter
                        .summarize_and_prepare(
                            &scope,
                            Some("model_input_active_threshold"),
                            &cancellation,
                        )
                        .await
                    {
                        Err(MemoryEngineContextError::Cancelled) => {
                            return Ok(cancelled_execution(assessments));
                        }
                        result => result?,
                    };
                }
            }
            _ => Err(ModelStepExecutorError::ContextStrategyMismatch),
        }
    }
}

fn build_request(
    run: &LocalAgentRun,
    request_id: String,
    input: Value,
    step: &LocalAgentProfileStep,
) -> ModelGatewayRequest {
    ModelGatewayRequest {
        request_id,
        model_config_id: run.model_config_id.clone(),
        model_config_revision: run.model_config_revision,
        protocol: run.model_runtime_snapshot.protocol,
        input,
        tools: step.tools.clone(),
        instructions: step.instructions.clone(),
        parameters: ModelGatewayParameters {
            maximum_output_tokens: step.maximum_output_tokens,
            reasoning_effort: step.reasoning_effort.clone(),
            temperature: step.temperature,
            native_compaction_threshold: step.native_compaction_threshold,
        },
    }
}

fn merge_input_items(context: &[Value], overlay: &[Value]) -> Vec<Value> {
    let mut input = Vec::with_capacity(context.len() + overlay.len());
    input.extend(context.iter().cloned());
    input.extend(overlay.iter().cloned());
    input
}

fn validate_json_items(label: &str, items: &[Value]) -> Result<(), ModelStepExecutorError> {
    if items.iter().any(|item| !item.is_object()) {
        return invalid_profile(format!("{label} items must be JSON objects"));
    }
    if serde_json::to_vec(items)
        .map_err(|_| ModelStepExecutorError::InvalidProfile(format!("{label} is invalid JSON")))?
        .len()
        > MAX_MODEL_GATEWAY_JSON_BYTES
    {
        return invalid_profile(format!("{label} exceeds the gateway payload limit"));
    }
    Ok(())
}

fn invalid_profile<T>(message: impl Into<String>) -> Result<T, ModelStepExecutorError> {
    Err(ModelStepExecutorError::InvalidProfile(message.into()))
}

fn terminal_failure_result(output: &ModelGatewayOutput) -> ModelStepResult {
    let status = match output.terminal.status {
        ModelGatewayTerminalStatus::Completed => "completed",
        ModelGatewayTerminalStatus::Incomplete => "incomplete",
        ModelGatewayTerminalStatus::Failed => "failed",
    };
    ModelStepResult::Failed(json!({
        "reason": "model_terminal_not_completed",
        "status": status,
        "terminal_event": output.terminal.terminal_event,
        "response_id": output.terminal.response_id,
        "provider_request_id": output.terminal.provider_request_id,
        "provider_http_status": output.terminal.provider_http_status,
    }))
}

fn cancelled_execution(token_assessments: Vec<ModelInputTokenAssessment>) -> ExecutedModelStep {
    ExecutedModelStep {
        result: ModelStepResult::Cancelled,
        output: None,
        token_assessments,
        provider_context_commit: None,
    }
}

fn validate_result_size(result: &ModelStepResult) -> Result<(), ModelStepExecutorError> {
    let bytes = serde_json::to_vec(result).map_err(|_| ModelStepExecutorError::ResultTooLarge)?;
    if bytes.len() > MAX_BOUNDED_JSON_BYTES {
        return Err(ModelStepExecutorError::ResultTooLarge);
    }
    Ok(())
}

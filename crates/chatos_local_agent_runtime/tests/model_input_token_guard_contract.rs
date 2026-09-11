// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    ContextStrategy, ModelGatewayParameters, ModelGatewayRequest, ModelGatewayTokenCount,
    ModelProtocol, ModelRuntimeDescriptor,
};
use chatos_local_agent_runtime::{
    guard_model_input, ModelGatewayCallbacks, ModelGatewayClient, ModelGatewayClientError,
    ModelGatewayOutput, ModelInputTokenAction, ModelInputTokenGuardError, ModelInputTokenSource,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct CountingGateway {
    input_tokens: u64,
}

#[async_trait]
impl ModelGatewayClient for CountingGateway {
    async fn descriptor(
        &self,
        _access_token: &str,
        _model_config_id: &str,
        _cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        unreachable!("not used by token guard tests")
    }

    async fn stream(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        _request: ModelGatewayRequest,
        _callbacks: ModelGatewayCallbacks,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        unreachable!("not used by token guard tests")
    }

    async fn count_input_tokens(
        &self,
        _access_token: &str,
        _descriptor: &ModelRuntimeDescriptor,
        request: &ModelGatewayRequest,
        _cancellation: CancellationToken,
    ) -> Result<ModelGatewayTokenCount, ModelGatewayClientError> {
        Ok(ModelGatewayTokenCount {
            request_id: request.request_id.clone(),
            model_config_id: request.model_config_id.clone(),
            model_config_revision: request.model_config_revision,
            input_tokens: self.input_tokens,
        })
    }
}

#[tokio::test]
async fn provider_native_threshold_keeps_the_request_and_enables_native_compaction() {
    let descriptor = descriptor(ContextStrategy::ProviderNative, true);
    let request = request(&descriptor, Some(200_000));
    let assessment = guard_model_input(
        &CountingGateway {
            input_tokens: 200_000,
        },
        "user-token",
        &descriptor,
        &request,
        None,
        CancellationToken::new(),
    )
    .await
    .expect("guarded request");

    assert_eq!(assessment.source, ModelInputTokenSource::ProviderExact);
    assert_eq!(
        assessment.action,
        ModelInputTokenAction::ProceedWithProviderCompaction
    );
    assert_eq!(assessment.maximum_input_tokens, 368_000);
}

#[tokio::test]
async fn memory_engine_threshold_requires_summary_before_model_io() {
    let descriptor = descriptor(ContextStrategy::MemoryEngine, true);
    let request = request(&descriptor, None);
    let assessment = guard_model_input(
        &CountingGateway {
            input_tokens: 210_000,
        },
        "user-token",
        &descriptor,
        &request,
        Some(200_000),
        CancellationToken::new(),
    )
    .await
    .expect("guarded request");

    assert_eq!(
        assessment.action,
        ModelInputTokenAction::RequireMemoryEngineSummary
    );
}

#[tokio::test]
async fn exact_hard_limit_overflow_fails_without_sending_the_model_request() {
    let descriptor = descriptor(ContextStrategy::MemoryEngine, true);
    let request = request(&descriptor, None);
    let error = guard_model_input(
        &CountingGateway {
            input_tokens: 368_001,
        },
        "user-token",
        &descriptor,
        &request,
        Some(200_000),
        CancellationToken::new(),
    )
    .await
    .expect_err("hard limit");

    assert!(matches!(
        error,
        ModelInputTokenGuardError::HardLimitExceeded {
            input_tokens: 368_001,
            maximum_input_tokens: 368_000,
            count_source: ModelInputTokenSource::ProviderExact,
        }
    ));
}

#[tokio::test]
async fn models_without_exact_count_use_an_explicit_conservative_upper_bound() {
    let descriptor = descriptor(ContextStrategy::MemoryEngine, false);
    let request = request(&descriptor, None);
    let assessment = guard_model_input(
        &CountingGateway {
            input_tokens: u64::MAX,
        },
        "user-token",
        &descriptor,
        &request,
        Some(200_000),
        CancellationToken::new(),
    )
    .await
    .expect("conservative count");

    assert_eq!(
        assessment.source,
        ModelInputTokenSource::SerializedByteUpperBound
    );
    assert_eq!(assessment.action, ModelInputTokenAction::Proceed);
    assert!(assessment.input_tokens > 0);
}

#[test]
fn context_strategies_cannot_share_or_omit_their_threshold_policy() {
    let provider = descriptor(ContextStrategy::ProviderNative, true);
    let provider_request = request(&provider, Some(200_000));
    assert!(
        chatos_local_agent_runtime::ModelInputTokenPolicy::for_request(
            &provider,
            &provider_request,
            Some(200_000)
        )
        .is_err()
    );

    let memory = descriptor(ContextStrategy::MemoryEngine, true);
    let memory_request = request(&memory, None);
    assert!(
        chatos_local_agent_runtime::ModelInputTokenPolicy::for_request(
            &memory,
            &memory_request,
            None
        )
        .is_err()
    );
}

fn descriptor(strategy: ContextStrategy, exact_count: bool) -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
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
        supports_input_token_count: exact_count,
    }
}

fn request(
    descriptor: &ModelRuntimeDescriptor,
    native_compaction_threshold: Option<u64>,
) -> ModelGatewayRequest {
    ModelGatewayRequest {
        request_id: "request-1".to_string(),
        model_config_id: descriptor.model_config_id.clone(),
        model_config_revision: descriptor.revision,
        protocol: descriptor.protocol,
        input: json!([{"type": "message", "role": "user", "content": "hello"}]),
        tools: Vec::new(),
        instructions: Some("Answer clearly".to_string()),
        parameters: ModelGatewayParameters {
            maximum_output_tokens: 32_000,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold,
        },
    }
}

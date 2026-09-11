// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{ContextStrategy, ModelGatewayRequest, ModelRuntimeDescriptor};
use tokio_util::sync::CancellationToken;

use crate::{ModelGatewayClient, ModelGatewayClientError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelInputTokenSource {
    ProviderExact,
    SerializedByteUpperBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelInputTokenAction {
    Proceed,
    ProceedWithProviderCompaction,
    RequireMemoryEngineSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInputTokenAssessment {
    pub input_tokens: u64,
    pub source: ModelInputTokenSource,
    pub active_threshold_tokens: u64,
    pub maximum_input_tokens: u64,
    pub action: ModelInputTokenAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInputTokenPolicy {
    context_strategy: ContextStrategy,
    active_threshold_tokens: u64,
    maximum_input_tokens: u64,
}

impl ModelInputTokenPolicy {
    pub fn for_request(
        descriptor: &ModelRuntimeDescriptor,
        request: &ModelGatewayRequest,
        memory_engine_active_threshold: Option<u64>,
    ) -> Result<Self, ModelInputTokenGuardError> {
        request
            .validate_against(descriptor)
            .map_err(|error| ModelInputTokenGuardError::InvalidPolicy(error.to_string()))?;
        let maximum_input_tokens = descriptor
            .context_window_tokens
            .checked_sub(u64::from(request.parameters.maximum_output_tokens))
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                ModelInputTokenGuardError::InvalidPolicy(
                    "output reserve leaves no model input capacity".to_string(),
                )
            })?;
        let active_threshold_tokens = match descriptor.context_strategy {
            ContextStrategy::ProviderNative => {
                if memory_engine_active_threshold.is_some() {
                    return Err(ModelInputTokenGuardError::InvalidPolicy(
                        "provider-native context cannot use a Memory Engine threshold".to_string(),
                    ));
                }
                request
                    .parameters
                    .native_compaction_threshold
                    .ok_or_else(|| {
                        ModelInputTokenGuardError::InvalidPolicy(
                            "provider-native context requires its frozen compaction threshold"
                                .to_string(),
                        )
                    })?
            }
            ContextStrategy::MemoryEngine => memory_engine_active_threshold.ok_or_else(|| {
                ModelInputTokenGuardError::InvalidPolicy(
                    "Memory Engine context requires an explicit active threshold".to_string(),
                )
            })?,
        };
        if active_threshold_tokens == 0 || active_threshold_tokens > maximum_input_tokens {
            return Err(ModelInputTokenGuardError::InvalidPolicy(
                "active threshold must fit within the model input capacity".to_string(),
            ));
        }
        Ok(Self {
            context_strategy: descriptor.context_strategy,
            active_threshold_tokens,
            maximum_input_tokens,
        })
    }

    pub const fn active_threshold_tokens(self) -> u64 {
        self.active_threshold_tokens
    }

    pub const fn maximum_input_tokens(self) -> u64 {
        self.maximum_input_tokens
    }

    pub fn assess(
        self,
        input_tokens: u64,
        source: ModelInputTokenSource,
    ) -> Result<ModelInputTokenAssessment, ModelInputTokenGuardError> {
        if input_tokens > self.maximum_input_tokens {
            return Err(ModelInputTokenGuardError::HardLimitExceeded {
                input_tokens,
                maximum_input_tokens: self.maximum_input_tokens,
                count_source: source,
            });
        }
        let action = if input_tokens < self.active_threshold_tokens {
            ModelInputTokenAction::Proceed
        } else {
            match self.context_strategy {
                ContextStrategy::ProviderNative => {
                    ModelInputTokenAction::ProceedWithProviderCompaction
                }
                ContextStrategy::MemoryEngine => ModelInputTokenAction::RequireMemoryEngineSummary,
            }
        };
        Ok(ModelInputTokenAssessment {
            input_tokens,
            source,
            active_threshold_tokens: self.active_threshold_tokens,
            maximum_input_tokens: self.maximum_input_tokens,
            action,
        })
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelInputTokenGuardError {
    #[error("invalid model input token policy: {0}")]
    InvalidPolicy(String),
    #[error("exact model input token count failed: {0}")]
    ExactCount(#[from] ModelGatewayClientError),
    #[error(
        "model input uses {input_tokens} tokens ({count_source:?}); maximum input capacity is {maximum_input_tokens}"
    )]
    HardLimitExceeded {
        input_tokens: u64,
        maximum_input_tokens: u64,
        count_source: ModelInputTokenSource,
    },
    #[error("model request could not be serialized for conservative token counting")]
    Serialization,
}

pub async fn guard_model_input(
    gateway: &dyn ModelGatewayClient,
    access_token: &str,
    descriptor: &ModelRuntimeDescriptor,
    request: &ModelGatewayRequest,
    memory_engine_active_threshold: Option<u64>,
    cancellation: CancellationToken,
) -> Result<ModelInputTokenAssessment, ModelInputTokenGuardError> {
    let policy =
        ModelInputTokenPolicy::for_request(descriptor, request, memory_engine_active_threshold)?;
    let (input_tokens, source) = if descriptor.supports_input_token_count {
        let count = gateway
            .count_input_tokens(access_token, descriptor, request, cancellation)
            .await?;
        (count.input_tokens, ModelInputTokenSource::ProviderExact)
    } else {
        let serialized =
            serde_json::to_vec(request).map_err(|_| ModelInputTokenGuardError::Serialization)?;
        let input_tokens = u64::try_from(serialized.len())
            .map_err(|_| ModelInputTokenGuardError::Serialization)?;
        (
            input_tokens,
            ModelInputTokenSource::SerializedByteUpperBound,
        )
    };
    policy.assess(input_tokens, source)
}

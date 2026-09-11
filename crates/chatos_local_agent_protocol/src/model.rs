// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    require_identifier, require_json_with_limit, require_nonempty_bounded_text,
    require_serialized_with_limit, ContextStrategy, ProtocolError, MAX_MODEL_GATEWAY_JSON_BYTES,
    MAX_STREAM_DELTA_BYTES,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelProtocol {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelRuntimeDescriptor {
    pub model_config_id: String,
    pub revision: u64,
    pub provider: String,
    pub model: String,
    pub protocol: ModelProtocol,
    pub context_window_tokens: u64,
    pub maximum_output_tokens: u32,
    pub context_strategy: ContextStrategy,
    pub supports_streaming: bool,
    pub supports_native_compaction: bool,
    pub supports_input_token_count: bool,
}

impl ModelRuntimeDescriptor {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("model_config_id", self.model_config_id.as_str()),
            ("provider", self.provider.as_str()),
            ("model", self.model.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        if self.revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "model descriptor revision must be positive",
            });
        }
        if self.context_window_tokens == 0 || self.maximum_output_tokens == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "model token limits must be positive",
            });
        }
        if u64::from(self.maximum_output_tokens) > self.context_window_tokens {
            return Err(ProtocolError::InvalidState {
                reason: "maximum output tokens exceed the context window",
            });
        }
        if self.supports_native_compaction
            && (self.protocol != ModelProtocol::Responses
                || self.context_strategy != ContextStrategy::ProviderNative)
        {
            return Err(ProtocolError::InvalidState {
                reason: "native compaction requires Responses with provider-native context",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelGatewayParameters {
    pub maximum_output_tokens: u32,
    pub reasoning_effort: Option<String>,
    pub temperature: Option<f64>,
    pub native_compaction_threshold: Option<u64>,
}

impl ModelGatewayParameters {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.maximum_output_tokens == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "gateway maximum output tokens must be positive",
            });
        }
        if let Some(reasoning_effort) = &self.reasoning_effort {
            require_identifier("reasoning_effort", reasoning_effort)?;
        }
        if self
            .temperature
            .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
        {
            return Err(ProtocolError::InvalidState {
                reason: "gateway temperature must be finite and between zero and two",
            });
        }
        if self.native_compaction_threshold == Some(0) {
            return Err(ProtocolError::InvalidState {
                reason: "native compaction threshold must be positive",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelGatewayRequest {
    pub request_id: String,
    pub model_config_id: String,
    pub model_config_revision: u64,
    pub protocol: ModelProtocol,
    pub input: Value,
    pub tools: Vec<Value>,
    pub instructions: Option<String>,
    pub parameters: ModelGatewayParameters,
}

impl ModelGatewayRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("request_id", &self.request_id)?;
        require_identifier("model_config_id", &self.model_config_id)?;
        if self.model_config_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "gateway model configuration revision must be positive",
            });
        }
        require_json_with_limit("model_input", &self.input, MAX_MODEL_GATEWAY_JSON_BYTES)?;
        require_serialized_with_limit("model_tools", &self.tools, MAX_MODEL_GATEWAY_JSON_BYTES)?;
        if let Some(instructions) = &self.instructions {
            require_nonempty_bounded_text(
                "instructions",
                instructions,
                MAX_MODEL_GATEWAY_JSON_BYTES,
            )?;
        }
        self.parameters.validate()
    }

    pub fn validate_against(
        &self,
        descriptor: &ModelRuntimeDescriptor,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        descriptor.validate()?;
        if self.model_config_id != descriptor.model_config_id
            || self.model_config_revision != descriptor.revision
            || self.protocol != descriptor.protocol
        {
            return Err(ProtocolError::InvalidState {
                reason: "gateway request does not match the frozen model descriptor",
            });
        }
        if self.parameters.maximum_output_tokens > descriptor.maximum_output_tokens {
            return Err(ProtocolError::InvalidState {
                reason: "gateway output tokens exceed the frozen descriptor",
            });
        }
        if self.parameters.native_compaction_threshold.is_some()
            && !descriptor.supports_native_compaction
        {
            return Err(ProtocolError::InvalidState {
                reason: "gateway request enabled unsupported native compaction",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelGatewayTerminalStatus {
    Completed,
    Incomplete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelGatewayTerminal {
    pub status: ModelGatewayTerminalStatus,
    pub response_id: Option<String>,
    pub provider_request_id: Option<String>,
    pub provider_terminal_event: String,
    pub provider_http_status: u16,
    pub usage: Option<Value>,
    pub output_items: Vec<Value>,
    pub incomplete_details: Option<Value>,
    pub provider_error: Option<Value>,
}

impl ModelGatewayTerminal {
    pub fn validate(&self, protocol: ModelProtocol) -> Result<(), ProtocolError> {
        require_identifier("provider_terminal_event", &self.provider_terminal_event)?;
        if let Some(response_id) = &self.response_id {
            require_identifier("response_id", response_id)?;
        }
        if let Some(request_id) = &self.provider_request_id {
            require_identifier("provider_request_id", request_id)?;
        }
        if self.provider_http_status == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "provider HTTP status must be positive",
            });
        }
        if protocol == ModelProtocol::Responses && self.response_id.is_none() {
            return Err(ProtocolError::InvalidState {
                reason: "Responses terminal results require a response ID",
            });
        }
        let valid_details = match self.status {
            ModelGatewayTerminalStatus::Completed => {
                self.incomplete_details.is_none() && self.provider_error.is_none()
            }
            ModelGatewayTerminalStatus::Incomplete => {
                self.incomplete_details.is_some() && self.provider_error.is_none()
            }
            ModelGatewayTerminalStatus::Failed => self.provider_error.is_some(),
        };
        if !valid_details {
            return Err(ProtocolError::InvalidState {
                reason: "gateway terminal status and details are inconsistent",
            });
        }
        for (field, payload) in [
            ("usage", self.usage.as_ref()),
            ("incomplete_details", self.incomplete_details.as_ref()),
            ("provider_error", self.provider_error.as_ref()),
        ] {
            if let Some(payload) = payload {
                require_json_with_limit(field, payload, MAX_MODEL_GATEWAY_JSON_BYTES)?;
            }
        }
        require_serialized_with_limit(
            "output_items",
            &self.output_items,
            MAX_MODEL_GATEWAY_JSON_BYTES,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelGatewayStreamEvent {
    ContentDelta { delta: String },
    ReasoningDelta { delta: String },
    OutputItem { item: Value },
    Terminal { terminal: ModelGatewayTerminal },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelGatewayStreamEnvelope {
    pub request_id: String,
    pub sequence: u64,
    pub protocol: ModelProtocol,
    pub event: ModelGatewayStreamEvent,
}

impl ModelGatewayStreamEnvelope {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("request_id", &self.request_id)?;
        if self.sequence == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "gateway stream sequence must be positive",
            });
        }
        match &self.event {
            ModelGatewayStreamEvent::ContentDelta { delta }
            | ModelGatewayStreamEvent::ReasoningDelta { delta } => {
                require_nonempty_bounded_text("stream_delta", delta, MAX_STREAM_DELTA_BYTES)
            }
            ModelGatewayStreamEvent::OutputItem { item } => {
                require_json_with_limit("output_item", item, MAX_MODEL_GATEWAY_JSON_BYTES)
            }
            ModelGatewayStreamEvent::Terminal { terminal } => terminal.validate(self.protocol),
        }
    }
}

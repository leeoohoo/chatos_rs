// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{
    ModelGatewayStreamEnvelope, ModelGatewayStreamEvent, ModelGatewayTerminal, ModelProtocol,
    ProtocolError, MAX_MODEL_GATEWAY_JSON_BYTES,
};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelGatewayOutput {
    pub content: String,
    pub reasoning: String,
    pub output_items: Vec<Value>,
    pub terminal: ModelGatewayTerminal,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelGatewayStreamError {
    #[error("invalid model gateway event: {0}")]
    InvalidEvent(#[from] ProtocolError),
    #[error("gateway stream request mismatch: expected {expected}, received {actual}")]
    RequestMismatch { expected: String, actual: String },
    #[error("gateway stream protocol changed during one request")]
    ProtocolMismatch,
    #[error("gateway stream sequence mismatch: expected {expected}, received {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },
    #[error("gateway stream emitted data after its terminal event")]
    EventAfterTerminal,
    #[error("gateway stream ended without a formal terminal event")]
    MissingTerminal,
}

#[derive(Debug, Clone)]
pub struct ModelGatewayStreamAccumulator {
    request_id: String,
    protocol: ModelProtocol,
    next_sequence: u64,
    content: String,
    reasoning: String,
    output_items: Vec<Value>,
    output_items_bytes: usize,
    terminal: Option<ModelGatewayTerminal>,
}

impl ModelGatewayStreamAccumulator {
    pub fn new(request_id: impl Into<String>, protocol: ModelProtocol) -> Self {
        Self {
            request_id: request_id.into(),
            protocol,
            next_sequence: 1,
            content: String::new(),
            reasoning: String::new(),
            output_items: Vec::new(),
            output_items_bytes: 2,
            terminal: None,
        }
    }

    pub fn accept(
        &mut self,
        envelope: ModelGatewayStreamEnvelope,
    ) -> Result<(), ModelGatewayStreamError> {
        envelope.validate()?;
        if envelope.request_id != self.request_id {
            return Err(ModelGatewayStreamError::RequestMismatch {
                expected: self.request_id.clone(),
                actual: envelope.request_id,
            });
        }
        if envelope.protocol != self.protocol {
            return Err(ModelGatewayStreamError::ProtocolMismatch);
        }
        if envelope.sequence != self.next_sequence {
            return Err(ModelGatewayStreamError::SequenceMismatch {
                expected: self.next_sequence,
                actual: envelope.sequence,
            });
        }
        if self.terminal.is_some() {
            return Err(ModelGatewayStreamError::EventAfterTerminal);
        }
        self.next_sequence =
            self.next_sequence
                .checked_add(1)
                .ok_or(ModelGatewayStreamError::SequenceMismatch {
                    expected: u64::MAX,
                    actual: envelope.sequence,
                })?;
        match envelope.event {
            ModelGatewayStreamEvent::ContentDelta { delta } => {
                append_bounded("content", &mut self.content, &delta)?;
            }
            ModelGatewayStreamEvent::ReasoningDelta { delta } => {
                append_bounded("reasoning", &mut self.reasoning, &delta)?;
            }
            ModelGatewayStreamEvent::OutputItem { item } => {
                let item_bytes = serde_json::to_vec(&item)
                    .map_err(|_| ProtocolError::InvalidJson {
                        field: "output_item",
                    })?
                    .len();
                let next_bytes = self
                    .output_items_bytes
                    .checked_add(item_bytes.saturating_add(1))
                    .ok_or(ProtocolError::PayloadTooLarge {
                        field: "output_items",
                        bytes: usize::MAX,
                        maximum: MAX_MODEL_GATEWAY_JSON_BYTES,
                    })?;
                if next_bytes > MAX_MODEL_GATEWAY_JSON_BYTES {
                    return Err(ProtocolError::PayloadTooLarge {
                        field: "output_items",
                        bytes: next_bytes,
                        maximum: MAX_MODEL_GATEWAY_JSON_BYTES,
                    }
                    .into());
                }
                self.output_items_bytes = next_bytes;
                self.output_items.push(item);
            }
            ModelGatewayStreamEvent::Terminal { mut terminal } => {
                if terminal.output_items.is_empty() {
                    terminal.output_items = self.output_items.clone();
                } else if !self.output_items.is_empty()
                    && terminal.output_items != self.output_items
                {
                    return Err(ModelGatewayStreamError::InvalidEvent(
                        ProtocolError::InvalidState {
                            reason: "streamed output items differ from the terminal snapshot",
                        },
                    ));
                }
                terminal.validate(self.protocol)?;
                self.terminal = Some(*terminal);
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Result<ModelGatewayOutput, ModelGatewayStreamError> {
        let terminal = self
            .terminal
            .ok_or(ModelGatewayStreamError::MissingTerminal)?;
        Ok(ModelGatewayOutput {
            content: self.content,
            reasoning: self.reasoning,
            output_items: terminal.output_items.clone(),
            terminal,
        })
    }
}

fn append_bounded(
    field: &'static str,
    target: &mut String,
    delta: &str,
) -> Result<(), ModelGatewayStreamError> {
    let next_bytes =
        target
            .len()
            .checked_add(delta.len())
            .ok_or(ProtocolError::PayloadTooLarge {
                field,
                bytes: usize::MAX,
                maximum: MAX_MODEL_GATEWAY_JSON_BYTES,
            })?;
    if next_bytes > MAX_MODEL_GATEWAY_JSON_BYTES {
        return Err(ProtocolError::PayloadTooLarge {
            field,
            bytes: next_bytes,
            maximum: MAX_MODEL_GATEWAY_JSON_BYTES,
        }
        .into());
    }
    target.push_str(delta);
    Ok(())
}

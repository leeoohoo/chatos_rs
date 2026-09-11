// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Stateless Responses context retention following OpenAI's official
//! compaction contract:
//! <https://developers.openai.com/api/docs/guides/compaction>
//!
//! Provider items are opaque. This module only appends exact request/response
//! items and, after a formal response, drops items before the newest
//! `compaction` item. It never summarizes, rewrites, or interprets encrypted
//! provider state.

use chatos_local_agent_protocol::MAX_MODEL_GATEWAY_JSON_BYTES;
use serde_json::Value;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProviderNativeContextError {
    #[error("provider context generation must be positive")]
    InvalidGeneration,
    #[error("provider context generation must increase from {current} to a newer value")]
    NonIncreasingGeneration { current: u64 },
    #[error("provider context item at index {index} must be a JSON object")]
    InvalidItem { index: usize },
    #[error("provider context item at index {index} exceeds {maximum} bytes")]
    ItemTooLarge { index: usize, maximum: usize },
    #[error("provider context item at index {index} could not be serialized")]
    InvalidJson { index: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderNativeContextWindow {
    generation: u64,
    items: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderNativeContextCommit {
    pub generation: u64,
    pub retained_items: Vec<Value>,
    pub dropped_item_count: usize,
    pub newest_compaction_id: Option<String>,
}

impl ProviderNativeContextWindow {
    pub fn new(generation: u64, items: Vec<Value>) -> Result<Self, ProviderNativeContextError> {
        validate_generation(generation)?;
        validate_items(&items)?;
        Ok(Self { generation, items })
    }

    pub fn empty(generation: u64) -> Result<Self, ProviderNativeContextError> {
        Self::new(generation, Vec::new())
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn items(&self) -> &[Value] {
        self.items.as_slice()
    }

    /// Creates the exact input array for one provider request without mutating
    /// durable context. Callers commit the new input only after receiving a
    /// formal provider terminal.
    pub fn request_input(
        &self,
        new_input_items: &[Value],
    ) -> Result<Vec<Value>, ProviderNativeContextError> {
        validate_items(new_input_items)?;
        let mut input = Vec::with_capacity(self.items.len() + new_input_items.len());
        input.extend(self.items.iter().cloned());
        input.extend(new_input_items.iter().cloned());
        Ok(input)
    }

    /// Atomically advances the local provider context after one formal
    /// response. The response output is preserved verbatim. If one or more new
    /// compaction items exist, only the newest compaction item and items after
    /// it remain.
    pub fn commit_response(
        &mut self,
        new_input_items: &[Value],
        response_output_items: &[Value],
    ) -> Result<ProviderNativeContextCommit, ProviderNativeContextError> {
        validate_items(new_input_items)?;
        validate_items(response_output_items)?;
        let mut next = Vec::with_capacity(
            self.items.len() + new_input_items.len() + response_output_items.len(),
        );
        next.extend(self.items.iter().cloned());
        next.extend(new_input_items.iter().cloned());
        next.extend(response_output_items.iter().cloned());

        let newest_compaction_index = next.iter().rposition(is_compaction_item);
        let dropped_item_count = newest_compaction_index.unwrap_or(0);
        if dropped_item_count > 0 {
            next.drain(..dropped_item_count);
        }
        let newest_compaction_id = newest_compaction_index.and_then(|_| {
            next.first()
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });
        self.items = next.clone();
        Ok(ProviderNativeContextCommit {
            generation: self.generation,
            retained_items: next,
            dropped_item_count,
            newest_compaction_id,
        })
    }

    /// Starts a new provider generation from semantic items only. Provider
    /// opaque state from the previous generation is intentionally discarded.
    pub fn rebuild_generation(
        &mut self,
        generation: u64,
        semantic_items: Vec<Value>,
    ) -> Result<(), ProviderNativeContextError> {
        validate_generation(generation)?;
        if generation <= self.generation {
            return Err(ProviderNativeContextError::NonIncreasingGeneration {
                current: self.generation,
            });
        }
        validate_items(&semantic_items)?;
        self.generation = generation;
        self.items = semantic_items;
        Ok(())
    }
}

fn validate_generation(generation: u64) -> Result<(), ProviderNativeContextError> {
    if generation == 0 {
        Err(ProviderNativeContextError::InvalidGeneration)
    } else {
        Ok(())
    }
}

fn validate_items(items: &[Value]) -> Result<(), ProviderNativeContextError> {
    for (index, item) in items.iter().enumerate() {
        if !item.is_object() {
            return Err(ProviderNativeContextError::InvalidItem { index });
        }
        let bytes = serde_json::to_vec(item)
            .map_err(|_| ProviderNativeContextError::InvalidJson { index })?;
        if bytes.len() > MAX_MODEL_GATEWAY_JSON_BYTES {
            return Err(ProviderNativeContextError::ItemTooLarge {
                index,
                maximum: MAX_MODEL_GATEWAY_JSON_BYTES,
            });
        }
    }
    Ok(())
}

fn is_compaction_item(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("compaction")
}

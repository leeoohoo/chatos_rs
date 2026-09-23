// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::memory_context::{MemoryContextComposer, MemoryScope};
use crate::runtime::{AiRuntime, AiRuntimeOptions, AiSingleStepOutcome, AiSingleStepRequest};
#[cfg(feature = "local-agent-loop")]
use crate::runtime::{AiRuntimeResult, AiTurnReport};
use crate::traits::{ModelRequest, ModelRuntimeConfig, RuntimeRecordOptions, SaveRecordInput};

pub struct ContextualTurnRunner {
    runtime: AiRuntime,
    memory_composer: Option<MemoryContextComposer>,
}

#[derive(Clone)]
pub struct ContextualTurnRequest {
    pub model_request: ModelRequest,
    pub runtime_options: AiRuntimeOptions,
    pub memory_scope: Option<MemoryScope>,
    pub prefixed_input_items: Vec<Value>,
    pub current_input_items: Vec<Value>,
    pub user_record: Option<SaveRecordInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTurnSpec {
    pub model_config: ModelRuntimeConfig,
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub caller_model: Option<String>,
    pub record_options: RuntimeRecordOptions,
    pub memory_scope: Option<MemoryScope>,
    pub prefixed_input_items: Vec<Value>,
    pub current_input_items: Vec<Value>,
    pub user_record: Option<SaveRecordInput>,
    pub tools: Vec<Value>,
}

impl ContextualTurnRunner {
    pub fn new(runtime: AiRuntime, memory_composer: Option<MemoryContextComposer>) -> Self {
        Self {
            runtime,
            memory_composer,
        }
    }

    pub fn runtime(&self) -> &AiRuntime {
        &self.runtime
    }

    pub async fn persist_external_tool_results(
        &self,
        runtime_options: &AiRuntimeOptions,
        tool_results: &[chatos_mcp_runtime::ToolResult],
    ) -> Result<(), String> {
        self.runtime
            .persist_external_tool_results(runtime_options, tool_results)
            .await
    }

    #[cfg(feature = "local-agent-loop")]
    pub async fn run_turn(
        &self,
        request: ContextualTurnRequest,
    ) -> Result<AiRuntimeResult, String> {
        let ContextualTurnRequest {
            model_request,
            runtime_options,
            memory_scope,
            prefixed_input_items,
            current_input_items,
            mut user_record,
        } = request;
        let contextual_input = build_contextual_input(
            self.memory_composer.as_ref(),
            memory_scope.as_ref(),
            prefixed_input_items.as_slice(),
            current_input_items.as_slice(),
            model_request.input.clone(),
            runtime_options.conversation_turn_id.as_deref(),
        )
        .await?;
        if let Some(user_record) = user_record.take() {
            self.runtime.save_record(user_record).await?;
        }

        let mut model_request = model_request;
        enable_openai_responses_protocol(&mut model_request);
        model_request.input = contextual_input;
        self.runtime.run_turn(model_request, runtime_options).await
    }

    pub async fn execute_once(
        &self,
        request: ContextualTurnRequest,
        iteration: usize,
        reason: impl Into<String>,
        model_attempt: usize,
    ) -> Result<AiSingleStepOutcome, String> {
        let ContextualTurnRequest {
            mut model_request,
            runtime_options,
            memory_scope,
            prefixed_input_items,
            current_input_items,
            user_record,
        } = request;
        let contextual_input = build_contextual_input(
            self.memory_composer.as_ref(),
            memory_scope.as_ref(),
            prefixed_input_items.as_slice(),
            current_input_items.as_slice(),
            model_request.input.clone(),
            runtime_options.conversation_turn_id.as_deref(),
        )
        .await?;
        if let Some(user_record) = user_record {
            self.runtime.save_record(user_record).await?;
        }
        enable_openai_responses_protocol(&mut model_request);
        model_request.input = contextual_input;
        let single_step = AiSingleStepRequest {
            model_request,
            runtime_options,
            iteration,
            reason: reason.into(),
            model_attempt,
            force_identity_encoding: false,
        };
        self.runtime.execute_once(single_step).await
    }

    #[cfg(feature = "local-agent-loop")]
    pub async fn run_turn_report(&self, request: ContextualTurnRequest) -> AiTurnReport {
        match self.run_turn(request).await {
            Ok(result) => result.into_report(),
            Err(err) => AiTurnReport::failed(err),
        }
    }
}

fn enable_openai_responses_protocol(request: &mut ModelRequest) {
    // Agent turns use one stable provider-owned Responses history for their
    // entire lifetime. Memory Engine is composed only at the turn boundary;
    // in-turn context management belongs to OpenAI Responses compaction.
    request.supports_responses = true;
}

impl RuntimeTurnSpec {
    pub fn new(model_config: ModelRuntimeConfig, conversation_id: impl Into<String>) -> Self {
        Self {
            model_config,
            conversation_id: conversation_id.into(),
            conversation_turn_id: None,
            caller_model: None,
            record_options: RuntimeRecordOptions::default(),
            memory_scope: None,
            prefixed_input_items: Vec::new(),
            current_input_items: Vec::new(),
            user_record: None,
            tools: Vec::new(),
        }
    }

    pub fn for_user_text(
        model_config: ModelRuntimeConfig,
        conversation_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::new(model_config, conversation_id)
            .with_current_input_items(vec![user_text_item(content)])
    }

    pub fn with_conversation_turn_id(mut self, conversation_turn_id: impl Into<String>) -> Self {
        self.conversation_turn_id = Some(conversation_turn_id.into());
        self
    }

    pub fn with_caller_model(mut self, caller_model: impl Into<String>) -> Self {
        self.caller_model = Some(caller_model.into());
        self
    }

    pub fn with_record_options(mut self, record_options: RuntimeRecordOptions) -> Self {
        self.record_options = record_options;
        self
    }

    pub fn with_memory_scope(mut self, memory_scope: Option<MemoryScope>) -> Self {
        self.memory_scope = memory_scope;
        self
    }

    pub fn with_prefixed_input_items(mut self, items: Vec<Value>) -> Self {
        self.prefixed_input_items = items;
        self
    }

    pub fn with_current_input_items(mut self, items: Vec<Value>) -> Self {
        self.current_input_items = items;
        self
    }

    pub fn with_user_record(mut self, user_record: Option<SaveRecordInput>) -> Self {
        self.user_record = user_record;
        self
    }

    pub fn with_tools(mut self, tools: Vec<Value>) -> Self {
        self.tools = tools;
        self
    }

    pub fn runtime_options(&self) -> AiRuntimeOptions {
        AiRuntimeOptions::new(
            Some(self.conversation_id.clone()),
            self.conversation_turn_id.clone(),
        )
        .with_caller_model(self.caller_model.clone())
        .with_caller_model_runtime(Some(self.model_config.to_tool_caller_model_runtime()))
        .with_record_options(self.record_options.clone())
    }

    pub fn into_contextual_turn_request(self) -> ContextualTurnRequest {
        let model_request = self
            .model_config
            .to_model_request(Value::Null, self.tools.clone());
        ContextualTurnRequest {
            model_request,
            runtime_options: self.runtime_options(),
            memory_scope: self.memory_scope,
            prefixed_input_items: self.prefixed_input_items,
            current_input_items: self.current_input_items,
            user_record: self.user_record,
        }
    }
}

impl ContextualTurnRequest {
    pub fn new(
        model_request: ModelRequest,
        runtime_options: AiRuntimeOptions,
        current_input_items: Vec<Value>,
    ) -> Self {
        Self {
            model_request,
            runtime_options,
            memory_scope: None,
            prefixed_input_items: Vec::new(),
            current_input_items,
            user_record: None,
        }
    }

    pub fn from_model_config(
        model_config: &ModelRuntimeConfig,
        runtime_options: AiRuntimeOptions,
        current_input_items: Vec<Value>,
    ) -> Self {
        Self::new(
            model_config.to_model_request(Value::Null, Vec::new()),
            runtime_options,
            current_input_items,
        )
    }

    pub fn for_user_text(
        model_config: &ModelRuntimeConfig,
        runtime_options: AiRuntimeOptions,
        content: impl Into<String>,
    ) -> Self {
        Self::from_model_config(model_config, runtime_options, vec![user_text_item(content)])
    }

    pub fn with_memory_scope(mut self, memory_scope: Option<MemoryScope>) -> Self {
        self.memory_scope = memory_scope;
        self
    }

    pub fn with_current_input_items(mut self, items: Vec<Value>) -> Self {
        self.current_input_items = items;
        self
    }

    pub fn with_prefixed_input_items(mut self, items: Vec<Value>) -> Self {
        self.prefixed_input_items = items;
        self
    }

    pub fn with_user_record(mut self, user_record: Option<SaveRecordInput>) -> Self {
        self.user_record = user_record;
        self
    }
}

pub async fn build_contextual_input(
    memory_composer: Option<&MemoryContextComposer>,
    memory_scope: Option<&MemoryScope>,
    prefixed_input_items: &[Value],
    current_input_items: &[Value],
    fallback_input: Value,
    excluded_memory_turn_id: Option<&str>,
) -> Result<Value, String> {
    let current_items = if current_input_items.is_empty() {
        input_value_to_items(fallback_input)
    } else {
        current_input_items.to_vec()
    };
    let has_durable_response_history = current_items.iter().any(is_durable_history_item);
    if has_durable_response_history {
        return Ok(Value::Array(current_items));
    }
    let memory_items = if let (Some(composer), Some(scope)) = (memory_composer, memory_scope) {
        composer
            .compose_input_items_excluding_turn(scope, excluded_memory_turn_id, None)
            .await?
    } else {
        Vec::new()
    };

    let mut items = prefixed_input_items.to_vec();
    items.extend(memory_items);
    items.extend(current_items);

    Ok(Value::Array(items))
}

fn is_responses_output_or_result_item(item: &Value) -> bool {
    matches!(
        item.get("type").and_then(Value::as_str),
        Some("reasoning")
            | Some("reasoning_summary")
            | Some("function_call")
            | Some("function_call_output")
            | Some("computer_call")
            | Some("computer_call_output")
            | Some("web_search_call")
            | Some("file_search_call")
    ) || (item.get("type").and_then(Value::as_str) == Some("message") && item.get("id").is_some())
}

fn is_durable_history_item(item: &Value) -> bool {
    is_responses_output_or_result_item(item)
        || matches!(
            item.get("role").and_then(Value::as_str),
            Some("assistant" | "tool")
        )
}

pub fn input_value_to_items(input: Value) -> Vec<Value> {
    match input {
        Value::Array(items) => items,
        Value::String(text) => vec![message_item("user", Value::String(text))],
        Value::Null => Vec::new(),
        other => vec![message_item("user", other)],
    }
}

pub fn user_text_item(content: impl Into<String>) -> Value {
    message_item("user", Value::String(content.into()))
}

pub fn message_item(role: &str, content: Value) -> Value {
    json!({
        "role": role,
        "content": content
    })
}

include!("turn_part01.rs");

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_ai_runtime::{
    AiRuntime, AiRuntimeResult, McpRuntimeToolExecutor, MemoryContextComposer,
    MemoryContextOverflowRecovery, MemoryRecordWriter, MemoryScope, ModelRuntimeConfig,
    RuntimeRecordOptions, RuntimeTurnSpec, SaveRecordInput, ToolExecutor,
};
use serde_json::Value;

use super::{AgentError, SystemAgentDefinition};
use crate::DEFAULT_AGENT_MAX_ITERATIONS;

#[derive(Clone)]
pub struct AgentTurnMemory {
    pub composer: MemoryContextComposer,
    pub writer: Arc<dyn MemoryRecordWriter>,
    pub scope: MemoryScope,
    pub conversation_id: String,
}

impl AgentTurnMemory {
    pub fn new<W>(
        composer: MemoryContextComposer,
        writer: W,
        scope: MemoryScope,
        conversation_id: impl Into<String>,
    ) -> Self
    where
        W: MemoryRecordWriter + 'static,
    {
        Self {
            composer,
            writer: Arc::new(writer),
            scope,
            conversation_id: conversation_id.into(),
        }
    }

    pub fn from_writer_arc(
        composer: MemoryContextComposer,
        writer: Arc<dyn MemoryRecordWriter>,
        scope: MemoryScope,
        conversation_id: impl Into<String>,
    ) -> Self {
        Self {
            composer,
            writer,
            scope,
            conversation_id: conversation_id.into(),
        }
    }
}

pub struct AgentTurnRequest {
    pub model_config: ModelRuntimeConfig,
    pub conversation_id: String,
    pub run_id: String,
    pub prompt: String,
    pub metadata: Value,
    pub tool_executor: Option<Arc<dyn ToolExecutor>>,
    pub memory: Option<AgentTurnMemory>,
    pub max_iterations: Option<usize>,
}

impl AgentTurnRequest {
    pub fn new(
        model_config: ModelRuntimeConfig,
        conversation_id: impl Into<String>,
        run_id: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            model_config,
            conversation_id: conversation_id.into(),
            run_id: run_id.into(),
            prompt: prompt.into(),
            metadata: Value::Null,
            tool_executor: None,
            memory: None,
            max_iterations: None,
        }
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_tool_executor<T>(mut self, tool_executor: T) -> Self
    where
        T: ToolExecutor + 'static,
    {
        self.tool_executor = Some(Arc::new(tool_executor));
        self
    }

    pub fn with_tool_executor_arc(mut self, tool_executor: Arc<dyn ToolExecutor>) -> Self {
        self.tool_executor = Some(tool_executor);
        self
    }

    pub fn with_mcp_executor(mut self, executor: chatos_mcp_runtime::McpExecutor) -> Self {
        self.tool_executor = Some(Arc::new(McpRuntimeToolExecutor::new(executor)));
        self
    }

    pub fn with_memory(mut self, memory: Option<AgentTurnMemory>) -> Self {
        self.memory = memory;
        self
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AgentExecutor;

impl AgentExecutor {
    pub const fn new() -> Self {
        Self
    }

    pub async fn run<A>(
        &self,
        agent: &A,
        request: AgentTurnRequest,
    ) -> Result<AiRuntimeResult, AgentError>
    where
        A: SystemAgentDefinition,
    {
        let key = agent.descriptor().key.as_str();
        let model_config = agent.configure_model(request.model_config);
        let caller_model = model_config.model.clone();
        let max_iterations = request
            .max_iterations
            .unwrap_or(DEFAULT_AGENT_MAX_ITERATIONS);
        let memory_scope = request.memory.as_ref().map(|memory| memory.scope.clone());
        let user_record = request.memory.as_ref().map(|memory| {
            SaveRecordInput::user_message(memory.conversation_id.clone(), request.prompt.clone())
                .with_conversation_turn_id(request.run_id.clone())
                .with_message_mode(agent.message_mode())
                .with_message_source(agent.message_source())
                .with_metadata(request.metadata.clone())
        });
        let record_options = RuntimeRecordOptions::persist_all()
            .with_assistant_message_mode(agent.message_mode())
            .with_assistant_message_source(agent.message_source())
            .with_assistant_metadata(request.metadata.clone())
            .with_tool_message_mode(agent.message_mode())
            .with_tool_message_source(agent.message_source())
            .with_tool_metadata(request.metadata.clone());

        let runtime = AiRuntime::new(request.tool_executor)
            .with_max_iterations(max_iterations)
            .with_record_writer(request.memory.as_ref().map(|memory| memory.writer.clone()));
        let runner = chatos_ai_runtime::ContextualTurnRunner::new(
            runtime,
            request
                .memory
                .as_ref()
                .map(|memory| memory.composer.clone()),
        )
        .with_context_overflow_recovery(Some(
            MemoryContextOverflowRecovery::new()
                .with_trigger_reason(agent.context_overflow_trigger()),
        ));

        let spec =
            RuntimeTurnSpec::for_user_text(model_config, request.conversation_id, request.prompt)
                .with_conversation_turn_id(request.run_id)
                .with_caller_model(caller_model)
                .with_record_options(record_options)
                .with_memory_scope(memory_scope)
                .with_user_record(user_record);

        runner
            .run_turn(spec.into_contextual_turn_request())
            .await
            .map_err(|error| AgentError::execution(key, error))
    }
}

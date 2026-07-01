// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use chatos_mcp_runtime::{BuiltinMcpPromptLocale, McpExecutor, McpExecutorBuilder};

use crate::builder::AiRuntimeBuilder;
use crate::runtime::MemoryContextOverflowRecovery;
use crate::traits::{MemoryRecordWriter, ToolExecutor};

use super::{TaskBuiltinMcpPromptMode, TaskRuntime};

pub struct TaskRuntimeBuilder {
    ai_builder: AiRuntimeBuilder,
    mcp_executor: Option<McpExecutor>,
    builtin_prompt_locale: BuiltinMcpPromptLocale,
    builtin_prompt_mode: TaskBuiltinMcpPromptMode,
}

impl TaskRuntimeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_ai_builder(mut self, ai_builder: AiRuntimeBuilder) -> Self {
        self.ai_builder = ai_builder;
        self
    }

    pub fn with_mcp_executor(mut self, mcp_executor: McpExecutor) -> Self {
        self.mcp_executor = Some(mcp_executor);
        self
    }

    pub async fn with_initialized_mcp_executor_builder(
        self,
        builder: McpExecutorBuilder,
    ) -> Result<Self, String> {
        Ok(self.with_mcp_executor(builder.build_initialized().await?))
    }

    pub fn with_builtin_only_mcp_executor_builder(
        self,
        builder: McpExecutorBuilder,
    ) -> Result<Self, String> {
        Ok(self.with_mcp_executor(builder.build_builtin_only()?))
    }

    pub fn with_builtin_prompt_locale(mut self, locale: BuiltinMcpPromptLocale) -> Self {
        self.builtin_prompt_locale = locale;
        self
    }

    pub fn with_builtin_prompt_mode(mut self, mode: TaskBuiltinMcpPromptMode) -> Self {
        self.builtin_prompt_mode = mode;
        self
    }

    pub fn with_tool_executor<T>(mut self, tool_executor: T) -> Self
    where
        T: ToolExecutor + 'static,
    {
        self.ai_builder = self.ai_builder.with_tool_executor(tool_executor);
        self
    }

    pub fn with_tool_executor_arc(mut self, tool_executor: Arc<dyn ToolExecutor>) -> Self {
        self.ai_builder = self.ai_builder.with_tool_executor_arc(tool_executor);
        self
    }

    pub fn with_record_writer<T>(mut self, record_writer: T) -> Self
    where
        T: MemoryRecordWriter + 'static,
    {
        self.ai_builder = self.ai_builder.with_record_writer(record_writer);
        self
    }

    pub fn with_record_writer_arc(mut self, record_writer: Arc<dyn MemoryRecordWriter>) -> Self {
        self.ai_builder = self.ai_builder.with_record_writer_arc(record_writer);
        self
    }

    pub fn with_memory_engine_record_writer_direct(
        mut self,
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
        scope: crate::memory_context::MemoryRecordScope,
    ) -> Result<Self, String> {
        self.ai_builder = self
            .ai_builder
            .with_memory_engine_record_writer_direct(base_url, timeout, source_id, scope)?;
        Ok(self)
    }

    pub fn with_memory_composer(
        mut self,
        memory_composer: crate::memory_context::MemoryContextComposer,
    ) -> Self {
        self.ai_builder = self.ai_builder.with_memory_composer(memory_composer);
        self
    }

    pub fn with_memory_composer_direct(
        mut self,
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
    ) -> Result<Self, String> {
        self.ai_builder = self
            .ai_builder
            .with_memory_composer_direct(base_url, timeout, source_id)?;
        Ok(self)
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.ai_builder = self.ai_builder.with_max_iterations(max_iterations);
        self
    }

    pub fn with_context_overflow_recovery(
        mut self,
        context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
    ) -> Self {
        self.ai_builder = self
            .ai_builder
            .with_context_overflow_recovery(context_overflow_recovery);
        self
    }

    pub fn build(self) -> TaskRuntime {
        let mcp_executor_for_runtime = self.mcp_executor.clone();
        let ai_builder = if let Some(executor) = mcp_executor_for_runtime {
            self.ai_builder.with_mcp_executor(executor)
        } else {
            self.ai_builder
        };
        TaskRuntime {
            runner: ai_builder.build_contextual_turn_runner(),
            mcp_executor: self.mcp_executor,
            builtin_prompt_locale: self.builtin_prompt_locale,
            builtin_prompt_mode: self.builtin_prompt_mode,
        }
    }
}

impl Default for TaskRuntimeBuilder {
    fn default() -> Self {
        Self {
            ai_builder: AiRuntimeBuilder::new(),
            mcp_executor: None,
            builtin_prompt_locale: BuiltinMcpPromptLocale::default(),
            builtin_prompt_mode: TaskBuiltinMcpPromptMode::default(),
        }
    }
}

use std::sync::Arc;
use std::time::Duration;

use crate::mcp_executor::McpRuntimeToolExecutor;
use crate::memory_context::{MemoryContextComposer, MemoryEngineRecordWriter, MemoryRecordScope};
use crate::runtime::{AiRuntime, MemoryContextOverflowRecovery};
use crate::traits::{MemoryRecordWriter, ToolExecutor};
use crate::turn::ContextualTurnRunner;

#[derive(Default)]
pub struct AiRuntimeBuilder {
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    record_writer: Option<Arc<dyn MemoryRecordWriter>>,
    memory_composer: Option<MemoryContextComposer>,
    max_iterations: Option<usize>,
    context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
}

impl AiRuntimeBuilder {
    pub fn new() -> Self {
        Self::default()
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

    pub fn with_record_writer<T>(mut self, record_writer: T) -> Self
    where
        T: MemoryRecordWriter + 'static,
    {
        self.record_writer = Some(Arc::new(record_writer));
        self
    }

    pub fn with_record_writer_arc(mut self, record_writer: Arc<dyn MemoryRecordWriter>) -> Self {
        self.record_writer = Some(record_writer);
        self
    }

    pub fn with_memory_engine_record_writer_direct(
        self,
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
        scope: MemoryRecordScope,
    ) -> Result<Self, String> {
        let writer = MemoryEngineRecordWriter::new_direct(base_url, timeout, source_id, scope)?;
        Ok(self.with_record_writer(writer))
    }

    pub fn with_memory_composer(mut self, memory_composer: MemoryContextComposer) -> Self {
        self.memory_composer = Some(memory_composer);
        self
    }

    pub fn with_memory_composer_direct(
        self,
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
    ) -> Result<Self, String> {
        let composer = MemoryContextComposer::new_direct(base_url, timeout, source_id)?;
        Ok(self.with_memory_composer(composer))
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }

    pub fn with_context_overflow_recovery(
        mut self,
        context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
    ) -> Self {
        self.context_overflow_recovery = context_overflow_recovery;
        self
    }

    pub fn build_runtime(self) -> AiRuntime {
        let mut runtime = AiRuntime::new(self.tool_executor).with_record_writer(self.record_writer);
        if let Some(max_iterations) = self.max_iterations {
            runtime = runtime.with_max_iterations(max_iterations);
        }
        runtime
    }

    pub fn build_contextual_turn_runner(self) -> ContextualTurnRunner {
        let memory_composer = self.memory_composer.clone();
        let context_overflow_recovery = self.context_overflow_recovery.clone();
        ContextualTurnRunner::new(self.build_runtime(), memory_composer)
            .with_context_overflow_recovery(context_overflow_recovery)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::AiRuntime;

    #[test]
    fn builder_accepts_shared_mcp_executor_and_builds_contextual_runner() {
        let executor = chatos_mcp_runtime::McpExecutor::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            chatos_mcp_runtime::BuiltinToolRegistry::new(),
        );
        let runner = AiRuntime::builder()
            .with_mcp_executor(executor)
            .with_memory_composer_direct(
                "http://127.0.0.1:1",
                Duration::from_millis(100),
                "task_runner",
            )
            .expect("composer")
            .with_max_iterations(3)
            .build_contextual_turn_runner();

        let _runtime = runner.runtime();
    }
}

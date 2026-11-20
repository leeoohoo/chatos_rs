// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use crate::memory_context::MemoryContextComposer;
use crate::runtime::AiRuntime;
use crate::traits::MemoryRecordWriter;
use crate::turn::ContextualTurnRunner;

#[derive(Default)]
pub struct AiRuntimeBuilder {
    record_writer: Option<Arc<dyn MemoryRecordWriter>>,
    memory_composer: Option<MemoryContextComposer>,
    max_iterations: Option<usize>,
    request_read_timeout: Option<Duration>,
}

impl AiRuntimeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_record_writer<T>(mut self, record_writer: T) -> Self
    where
        T: MemoryRecordWriter + 'static,
    {
        self.record_writer = Some(Arc::new(record_writer));
        self
    }

    pub fn with_memory_composer(mut self, memory_composer: MemoryContextComposer) -> Self {
        self.memory_composer = Some(memory_composer);
        self
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }

    pub fn with_request_read_timeout(mut self, read_timeout: Duration) -> Self {
        self.request_read_timeout = Some(read_timeout);
        self
    }

    pub fn build_runtime(self) -> AiRuntime {
        let mut runtime = AiRuntime::default().with_record_writer(self.record_writer);
        if let Some(max_iterations) = self.max_iterations {
            runtime = runtime.with_max_iterations(max_iterations);
        }
        if let Some(read_timeout) = self.request_read_timeout {
            runtime = runtime.with_request_read_timeout(read_timeout);
        }
        runtime
    }

    pub fn build_contextual_turn_runner(self) -> ContextualTurnRunner {
        let memory_composer = self.memory_composer.clone();
        ContextualTurnRunner::new(self.build_runtime(), memory_composer)
    }
}

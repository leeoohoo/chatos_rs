// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use chatos_mcp_runtime::{BuiltinMcpPromptBuildResult, BuiltinMcpPromptLocale, McpExecutor};

use crate::runtime::AiRuntimeOptions;
use crate::turn::ContextualTurnRunner;

mod config;
mod execution;
mod memory;
mod report;
mod runtime_builder;
mod spec;

pub use self::config::{TaskMcpInitMode, TaskRuntimeConfig};
pub use self::execution::TaskRunExecution;
pub use self::memory::TaskMemoryRuntimeConfig;
pub use self::report::TaskRunReport;
pub use self::runtime_builder::TaskRuntimeBuilder;
pub use self::spec::TaskRunSpec;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskBuiltinMcpPromptMode {
    Configured,
    Effective,
}

impl Default for TaskBuiltinMcpPromptMode {
    fn default() -> Self {
        Self::Effective
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBuiltinMcpPromptSnapshot {
    pub mode: TaskBuiltinMcpPromptMode,
    pub locale: BuiltinMcpPromptLocale,
    pub build: BuiltinMcpPromptBuildResult,
}

pub struct TaskRuntime {
    runner: ContextualTurnRunner,
    mcp_executor: Option<McpExecutor>,
    builtin_prompt_locale: BuiltinMcpPromptLocale,
    builtin_prompt_mode: TaskBuiltinMcpPromptMode,
}

impl TaskRuntime {
    pub fn builder() -> TaskRuntimeBuilder {
        TaskRuntimeBuilder::new()
    }

    pub fn new(runner: ContextualTurnRunner) -> Self {
        Self {
            runner,
            mcp_executor: None,
            builtin_prompt_locale: BuiltinMcpPromptLocale::default(),
            builtin_prompt_mode: TaskBuiltinMcpPromptMode::default(),
        }
    }

    pub fn runner(&self) -> &ContextualTurnRunner {
        &self.runner
    }

    pub fn mcp_executor(&self) -> Option<&McpExecutor> {
        self.mcp_executor.as_ref()
    }

    pub fn builtin_prompt_locale(&self) -> BuiltinMcpPromptLocale {
        self.builtin_prompt_locale
    }

    pub fn builtin_prompt_mode(&self) -> TaskBuiltinMcpPromptMode {
        self.builtin_prompt_mode
    }

    pub fn prepare_spec(&self, spec: TaskRunSpec) -> TaskRunSpec {
        let Some(executor) = self.mcp_executor.as_ref() else {
            return spec;
        };
        match self.builtin_prompt_mode {
            TaskBuiltinMcpPromptMode::Configured => spec
                .with_configured_builtin_mcp_prompt_from_executor(
                    executor,
                    self.builtin_prompt_locale,
                ),
            TaskBuiltinMcpPromptMode::Effective => spec
                .with_effective_builtin_mcp_prompt_from_executor(
                    executor,
                    self.builtin_prompt_locale,
                ),
        }
    }

    pub async fn run_task_report(&self, spec: TaskRunSpec) -> TaskRunReport {
        self.runner.run_task_report(self.prepare_spec(spec)).await
    }

    pub async fn run_task_report_with_options(
        &self,
        spec: TaskRunSpec,
        runtime_options: AiRuntimeOptions,
    ) -> TaskRunReport {
        self.runner
            .run_task_report_with_options(self.prepare_spec(spec), runtime_options)
            .await
    }
}

impl ContextualTurnRunner {
    pub async fn run_task_report(&self, spec: TaskRunSpec) -> TaskRunReport {
        let task_id = spec.task_id.clone();
        let run_id = spec.run_id.clone();
        let model_config_id = spec.model_config_id.clone();
        let report = self
            .run_turn_report(spec.into_contextual_turn_request())
            .await;
        TaskRunReport::from_ai_report(task_id, run_id, model_config_id, report)
    }

    pub async fn run_task_report_with_options(
        &self,
        spec: TaskRunSpec,
        runtime_options: AiRuntimeOptions,
    ) -> TaskRunReport {
        let task_id = spec.task_id.clone();
        let run_id = spec.run_id.clone();
        let model_config_id = spec.model_config_id.clone();
        let report = self
            .run_turn_report(spec.into_contextual_turn_request_with_options(runtime_options))
            .await;
        TaskRunReport::from_ai_report(task_id, run_id, model_config_id, report)
    }
}

#[cfg(test)]
mod tests;

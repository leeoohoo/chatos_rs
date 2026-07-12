// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{
    AiRuntimeOptions, ModelRuntimeConfig, RuntimeRecordOptions, SaveRecordInput, TaskRunExecution,
    TaskRunReport, TaskRunSpec, TaskRuntime, TaskRuntimeConfig,
};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity};

pub const TASK_RUNNER_AGENT: TaskRunnerAgent = TaskRunnerAgent;

#[derive(Debug, Default, Clone, Copy)]
pub struct TaskRunnerAgent;

impl AgentIdentity for TaskRunnerAgent {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(SystemAgentKey::TaskRunnerRunPhase)
    }
}

pub struct TaskRunnerRunSpecInput {
    pub task_id: String,
    pub run_id: String,
    pub model_config: ModelRuntimeConfig,
    pub model_config_id: String,
    pub prompt: String,
    pub metadata: Value,
    pub prefixed_input_items: Vec<Value>,
}

impl TaskRunnerRunSpecInput {
    pub fn new(
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        model_config: ModelRuntimeConfig,
        model_config_id: impl Into<String>,
        prompt: impl Into<String>,
        metadata: Value,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            run_id: run_id.into(),
            model_config,
            model_config_id: model_config_id.into(),
            prompt: prompt.into(),
            metadata,
            prefixed_input_items: Vec::new(),
        }
    }

    pub fn with_prefixed_input_items(mut self, items: Vec<Value>) -> Self {
        self.prefixed_input_items = items;
        self
    }
}

impl TaskRunnerAgent {
    pub fn build_run_spec(&self, input: TaskRunnerRunSpecInput) -> TaskRunSpec {
        let TaskRunnerRunSpecInput {
            task_id,
            run_id,
            model_config,
            model_config_id,
            prompt,
            metadata,
            prefixed_input_items,
        } = input;
        let record_options = self.record_options(metadata.clone());
        let user_record = SaveRecordInput::user_message(run_id.clone(), prompt.clone())
            .with_conversation_turn_id(run_id.clone())
            .with_message_mode(self.user_message_mode())
            .with_message_source(self.message_source())
            .with_metadata(metadata.clone());
        let mut spec = TaskRunSpec::new(task_id, run_id, model_config, prompt)
            .with_model_config_id(model_config_id)
            .with_metadata(Some(metadata))
            .with_record_options(record_options)
            .with_user_record(Some(user_record));
        if !prefixed_input_items.is_empty() {
            spec = spec.with_prefixed_input_items(prefixed_input_items);
        }
        spec
    }

    pub async fn run_report_with_runtime_options(
        &self,
        runtime_config: TaskRuntimeConfig,
        run_spec: TaskRunSpec,
        runtime: &TaskRuntime,
        runtime_options: AiRuntimeOptions,
    ) -> TaskRunReport {
        TaskRunExecution::new(runtime_config, run_spec)
            .run_report_with_runtime_options(runtime, runtime_options)
            .await
    }

    fn record_options(&self, metadata: Value) -> RuntimeRecordOptions {
        RuntimeRecordOptions::persist_all()
            .with_assistant_message_mode(self.assistant_message_mode())
            .with_assistant_message_source(self.message_source())
            .with_tool_message_mode(self.tool_message_mode())
            .with_tool_message_source(self.message_source())
            .with_assistant_metadata(metadata.clone())
            .with_tool_metadata(metadata)
    }

    fn user_message_mode(&self) -> &'static str {
        "task_run"
    }

    fn assistant_message_mode(&self) -> &'static str {
        "task_run"
    }

    fn tool_message_mode(&self) -> &'static str {
        "task_tool"
    }

    fn message_source(&self) -> &'static str {
        "task_runner"
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn model_config() -> ModelRuntimeConfig {
        ModelRuntimeConfig::openai_compatible(
            "http://localhost",
            "key",
            "model",
            "openai_compatible",
        )
    }

    #[test]
    fn builds_task_runner_record_contract() {
        let metadata = json!({
            "task_id": "task-1",
            "run_id": "run-1",
            "service": "task_runner_service",
        });
        let spec = TASK_RUNNER_AGENT.build_run_spec(
            TaskRunnerRunSpecInput::new(
                "task-1",
                "run-1",
                model_config(),
                "model-config-1",
                "do work",
                metadata.clone(),
            )
            .with_prefixed_input_items(vec![json!({"role": "system", "content": "context"})]),
        );

        assert_eq!(spec.model_config_id.as_deref(), Some("model-config-1"));
        assert_eq!(spec.metadata, Some(metadata.clone()));
        assert_eq!(spec.prefixed_input_items.len(), 1);
        assert_eq!(
            spec.record_options.assistant_message_mode.as_deref(),
            Some("task_run")
        );
        assert_eq!(
            spec.record_options.tool_message_mode.as_deref(),
            Some("task_tool")
        );
        let user_record = spec.user_record.expect("user record");
        assert_eq!(user_record.conversation_id, "run-1");
        assert_eq!(user_record.conversation_turn_id.as_deref(), Some("run-1"));
        assert_eq!(user_record.message_mode.as_deref(), Some("task_run"));
        assert_eq!(user_record.message_source.as_deref(), Some("task_runner"));
        assert_eq!(user_record.metadata, Some(metadata));
    }
}

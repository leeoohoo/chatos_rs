use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use chatos_mcp_runtime::{BuiltinMcpPromptLocale, McpExecutor};

use crate::memory_context::MemoryScope;
use crate::runtime::AiRuntimeOptions;
use crate::traits::{ModelRuntimeConfig, RuntimeRecordOptions, SaveRecordInput};
use crate::turn::{message_item, user_text_item, ContextualTurnRequest, RuntimeTurnSpec};

use super::{TaskBuiltinMcpPromptMode, TaskBuiltinMcpPromptSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunSpec {
    pub task_id: String,
    pub run_id: String,
    pub model_config_id: Option<String>,
    pub model_config: ModelRuntimeConfig,
    pub prompt: String,
    pub memory_scope: Option<MemoryScope>,
    pub record_options: RuntimeRecordOptions,
    pub prefixed_input_items: Vec<Value>,
    pub current_input_items: Vec<Value>,
    pub user_record: Option<SaveRecordInput>,
    pub tools: Vec<Value>,
    pub metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin_mcp_prompt: Option<TaskBuiltinMcpPromptSnapshot>,
}

impl TaskRunSpec {
    pub fn new(
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        model_config: ModelRuntimeConfig,
        prompt: impl Into<String>,
    ) -> Self {
        let task_id = task_id.into();
        let run_id = run_id.into();
        let prompt = prompt.into();
        let metadata = task_metadata(task_id.as_str(), run_id.as_str(), None);
        Self {
            task_id: task_id.clone(),
            run_id: run_id.clone(),
            model_config,
            prompt: prompt.clone(),
            model_config_id: None,
            memory_scope: None,
            record_options: task_record_options(metadata.clone()),
            prefixed_input_items: Vec::new(),
            current_input_items: vec![user_text_item(prompt.clone())],
            user_record: Some(
                SaveRecordInput::user_message(task_id, prompt)
                    .with_conversation_turn_id(run_id)
                    .with_message_mode("task_run")
                    .with_message_source("task_runner")
                    .with_metadata(metadata.clone()),
            ),
            tools: Vec::new(),
            metadata: Some(metadata),
            builtin_mcp_prompt: None,
        }
    }

    pub fn with_model_config_id(mut self, model_config_id: impl Into<String>) -> Self {
        let model_config_id = model_config_id.into();
        self.model_config_id = Some(model_config_id.clone());
        let metadata = task_metadata(
            self.task_id.as_str(),
            self.run_id.as_str(),
            Some(model_config_id.as_str()),
        );
        self.metadata = Some(metadata.clone());
        self.record_options = task_record_options(metadata.clone());
        if let Some(user_record) = self.user_record.take() {
            self.user_record = Some(user_record.with_metadata(metadata));
        }
        self
    }

    pub fn with_memory_scope(mut self, memory_scope: Option<MemoryScope>) -> Self {
        self.memory_scope = memory_scope;
        self
    }

    pub fn with_record_options(mut self, record_options: RuntimeRecordOptions) -> Self {
        self.record_options = record_options;
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

    pub fn with_metadata(mut self, metadata: Option<Value>) -> Self {
        self.metadata = metadata.clone();
        if let Some(metadata) = metadata {
            self.record_options = task_record_options(metadata.clone());
            if let Some(user_record) = self.user_record.take() {
                self.user_record = Some(user_record.with_metadata(metadata));
            }
        }
        self
    }

    pub fn with_builtin_mcp_prompt(mut self, prompt: impl Into<String>) -> Self {
        let prompt = prompt.into();
        if !prompt.trim().is_empty() {
            self.prefixed_input_items
                .insert(0, message_item("system", Value::String(prompt)));
        }
        self
    }

    pub fn with_optional_builtin_mcp_prompt(self, prompt: Option<String>) -> Self {
        match prompt {
            Some(prompt) => self.with_builtin_mcp_prompt(prompt),
            None => self,
        }
    }

    pub fn with_configured_builtin_mcp_prompt_from_executor(
        self,
        executor: &McpExecutor,
        locale: BuiltinMcpPromptLocale,
    ) -> Self {
        let snapshot = TaskBuiltinMcpPromptSnapshot {
            mode: TaskBuiltinMcpPromptMode::Configured,
            locale,
            build: executor.inspect_builtin_mcp_system_prompt(locale),
        };
        self.with_builtin_mcp_prompt_snapshot(snapshot)
    }

    pub fn with_effective_builtin_mcp_prompt_from_executor(
        self,
        executor: &McpExecutor,
        locale: BuiltinMcpPromptLocale,
    ) -> Self {
        let snapshot = TaskBuiltinMcpPromptSnapshot {
            mode: TaskBuiltinMcpPromptMode::Effective,
            locale,
            build: executor.inspect_effective_builtin_mcp_system_prompt(locale),
        };
        self.with_builtin_mcp_prompt_snapshot(snapshot)
    }

    pub fn with_builtin_mcp_prompt_snapshot(
        mut self,
        snapshot: TaskBuiltinMcpPromptSnapshot,
    ) -> Self {
        if let Some(previous_prompt) = self
            .builtin_mcp_prompt
            .as_ref()
            .and_then(|snapshot| snapshot.build.prompt.as_deref())
        {
            remove_prefixed_system_prompt(&mut self.prefixed_input_items, previous_prompt);
        }
        if let Some(prompt) = snapshot
            .build
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.prefixed_input_items
                .insert(0, message_item("system", Value::String(prompt.to_string())));
        }
        self.builtin_mcp_prompt = Some(snapshot);
        self
    }

    pub fn runtime_options(&self) -> AiRuntimeOptions {
        AiRuntimeOptions::new(Some(self.task_id.clone()), Some(self.run_id.clone()))
            .with_caller_model(Some(self.model_config.model.clone()))
            .with_caller_model_runtime(Some(self.model_config.to_tool_caller_model_runtime()))
            .with_record_options(self.record_options.clone())
    }

    pub fn into_runtime_turn_spec(self) -> RuntimeTurnSpec {
        RuntimeTurnSpec::new(self.model_config.clone(), self.task_id.clone())
            .with_conversation_turn_id(self.run_id.clone())
            .with_caller_model(self.model_config.model.clone())
            .with_record_options(self.record_options)
            .with_memory_scope(self.memory_scope)
            .with_prefixed_input_items(self.prefixed_input_items)
            .with_current_input_items(self.current_input_items)
            .with_user_record(self.user_record)
            .with_tools(self.tools)
    }

    pub fn into_contextual_turn_request(self) -> ContextualTurnRequest {
        self.into_runtime_turn_spec().into_contextual_turn_request()
    }

    pub fn into_contextual_turn_request_with_options(
        self,
        runtime_options: AiRuntimeOptions,
    ) -> ContextualTurnRequest {
        let mut request = self.into_contextual_turn_request();
        request.runtime_options = runtime_options;
        request
    }
}

fn task_record_options(metadata: Value) -> RuntimeRecordOptions {
    RuntimeRecordOptions::persist_all()
        .with_assistant_message_mode("task_run")
        .with_assistant_message_source("task_runner")
        .with_assistant_metadata(metadata.clone())
        .with_tool_message_mode("task_run")
        .with_tool_message_source("task_runner")
        .with_tool_metadata(metadata)
}

fn task_metadata(task_id: &str, run_id: &str, model_config_id: Option<&str>) -> Value {
    let mut metadata = json!({
        "task_id": task_id,
        "run_id": run_id,
    });
    if let Some(model_config_id) = model_config_id {
        metadata["model_config_id"] = Value::String(model_config_id.to_string());
    }
    metadata
}

fn remove_prefixed_system_prompt(items: &mut Vec<Value>, prompt: &str) {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return;
    }
    items.retain(|item| {
        let role = item.get("role").and_then(Value::as_str).unwrap_or("");
        let content = item.get("content").and_then(Value::as_str).unwrap_or("");
        !(role == "system" && content.trim() == prompt)
    });
}

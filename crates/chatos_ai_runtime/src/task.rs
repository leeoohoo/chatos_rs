use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use chatos_mcp_runtime::{
    builtin_servers_from_kinds, BuiltinMcpKind, BuiltinMcpPromptBuildResult,
    BuiltinMcpPromptLocale, BuiltinMcpServerOptions, McpBuiltinServer, McpExecutor,
    McpExecutorBuilder, McpHttpServer, McpStdioServer,
};

use crate::builder::AiRuntimeBuilder;
use crate::memory_context::{
    BestEffortMemoryRecordWriter, MemoryEngineRecordWriter, MemoryRecordScope, MemoryScope,
};
use crate::runtime::{AiRuntimeOptions, AiTurnReport, AiTurnStatus, MemoryContextOverflowRecovery};
use crate::traits::{
    MemoryRecordWriter, ModelRuntimeConfig, RuntimeRecordOptions, SaveRecordInput, ToolExecutor,
};
use crate::turn::{
    message_item, user_text_item, ContextualTurnRequest, ContextualTurnRunner, RuntimeTurnSpec,
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunReport {
    pub task_id: String,
    pub run_id: String,
    pub model_config_id: Option<String>,
    pub status: AiTurnStatus,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub error: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    pub completed_at: String,
}

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskMcpInitMode {
    Full,
    BuiltinOnly,
    Disabled,
}

impl Default for TaskMcpInitMode {
    fn default() -> Self {
        Self::Full
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRuntimeConfig {
    #[serde(default)]
    pub http_servers: Vec<McpHttpServer>,
    #[serde(default)]
    pub stdio_servers: Vec<McpStdioServer>,
    #[serde(default)]
    pub builtin_servers: Vec<McpBuiltinServer>,
    #[serde(default)]
    pub mcp_init_mode: TaskMcpInitMode,
    #[serde(default)]
    pub builtin_prompt_locale: BuiltinMcpPromptLocale,
    #[serde(default)]
    pub builtin_prompt_mode: TaskBuiltinMcpPromptMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_engine: Option<TaskMemoryRuntimeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunExecution {
    pub runtime_config: TaskRuntimeConfig,
    pub run_spec: TaskRunSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemoryRuntimeConfig {
    pub base_url: String,
    pub source_id: String,
    #[serde(default = "default_memory_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_memory_compose_context")]
    pub compose_context: bool,
    #[serde(default = "default_retry_on_context_overflow")]
    pub retry_on_context_overflow: bool,
    #[serde(default = "default_active_summary_poll_interval_ms")]
    pub active_summary_poll_interval_ms: u64,
    #[serde(default = "default_active_summary_poll_timeout_ms")]
    pub active_summary_poll_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_scope: Option<MemoryRecordScope>,
}

pub struct TaskRuntime {
    runner: ContextualTurnRunner,
    mcp_executor: Option<McpExecutor>,
    builtin_prompt_locale: BuiltinMcpPromptLocale,
    builtin_prompt_mode: TaskBuiltinMcpPromptMode,
}

pub struct TaskRuntimeBuilder {
    ai_builder: AiRuntimeBuilder,
    mcp_executor: Option<McpExecutor>,
    builtin_prompt_locale: BuiltinMcpPromptLocale,
    builtin_prompt_mode: TaskBuiltinMcpPromptMode,
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

impl TaskRuntimeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_http_servers<I>(mut self, servers: I) -> Self
    where
        I: IntoIterator<Item = McpHttpServer>,
    {
        self.http_servers.extend(servers);
        self
    }

    pub fn with_stdio_servers<I>(mut self, servers: I) -> Self
    where
        I: IntoIterator<Item = McpStdioServer>,
    {
        self.stdio_servers.extend(servers);
        self
    }

    pub fn with_builtin_servers<I>(mut self, servers: I) -> Self
    where
        I: IntoIterator<Item = McpBuiltinServer>,
    {
        self.builtin_servers.extend(servers);
        self
    }

    pub fn with_builtin_kinds<I>(self, kinds: I, options: &BuiltinMcpServerOptions) -> Self
    where
        I: IntoIterator<Item = BuiltinMcpKind>,
    {
        self.with_builtin_servers(builtin_servers_from_kinds(kinds, options))
    }

    pub fn with_mcp_init_mode(mut self, mode: TaskMcpInitMode) -> Self {
        self.mcp_init_mode = mode;
        self
    }

    pub fn with_builtin_prompt_locale(mut self, locale: BuiltinMcpPromptLocale) -> Self {
        self.builtin_prompt_locale = locale;
        self
    }

    pub fn with_builtin_prompt_mode(mut self, mode: TaskBuiltinMcpPromptMode) -> Self {
        self.builtin_prompt_mode = mode;
        self
    }

    pub fn with_max_iterations(mut self, max_iterations: Option<usize>) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    pub fn with_memory_engine(mut self, memory_engine: Option<TaskMemoryRuntimeConfig>) -> Self {
        self.memory_engine = memory_engine;
        self
    }

    pub fn to_mcp_executor_builder(&self) -> McpExecutorBuilder {
        McpExecutorBuilder::new()
            .with_http_servers(self.http_servers.clone())
            .with_stdio_servers(self.stdio_servers.clone())
            .with_builtin_servers(self.builtin_servers.clone())
    }

    pub fn apply_to_builder(&self, builder: TaskRuntimeBuilder) -> TaskRuntimeBuilder {
        let mut builder = builder
            .with_builtin_prompt_locale(self.builtin_prompt_locale)
            .with_builtin_prompt_mode(self.builtin_prompt_mode);
        if let Some(max_iterations) = self.max_iterations {
            builder = builder.with_max_iterations(max_iterations);
        }
        builder
    }

    pub fn try_apply_to_builder(
        &self,
        builder: TaskRuntimeBuilder,
    ) -> Result<TaskRuntimeBuilder, String> {
        let builder = self.apply_to_builder(builder);
        if let Some(memory_engine) = &self.memory_engine {
            memory_engine.apply_to_builder(builder)
        } else {
            Ok(builder)
        }
    }

    pub async fn build_runtime(&self) -> Result<TaskRuntime, String> {
        self.build_runtime_with_mcp_builder(self.to_mcp_executor_builder())
            .await
    }

    pub async fn build_runtime_with_mcp_builder(
        &self,
        mcp_builder: McpExecutorBuilder,
    ) -> Result<TaskRuntime, String> {
        let builder = self.try_apply_to_builder(TaskRuntimeBuilder::new())?;
        let builder = match self.mcp_init_mode {
            TaskMcpInitMode::Full => {
                builder
                    .with_initialized_mcp_executor_builder(mcp_builder)
                    .await?
            }
            TaskMcpInitMode::BuiltinOnly => {
                builder.with_builtin_only_mcp_executor_builder(mcp_builder)?
            }
            TaskMcpInitMode::Disabled => builder,
        };
        Ok(builder.build())
    }
}

impl TaskMemoryRuntimeConfig {
    pub fn new(base_url: impl Into<String>, source_id: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            source_id: source_id.into(),
            timeout_ms: default_memory_timeout_ms(),
            compose_context: default_memory_compose_context(),
            retry_on_context_overflow: default_retry_on_context_overflow(),
            active_summary_poll_interval_ms: default_active_summary_poll_interval_ms(),
            active_summary_poll_timeout_ms: default_active_summary_poll_timeout_ms(),
            record_scope: None,
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_compose_context(mut self, compose_context: bool) -> Self {
        self.compose_context = compose_context;
        self
    }

    pub fn with_record_scope(mut self, record_scope: Option<MemoryRecordScope>) -> Self {
        self.record_scope = record_scope;
        self
    }

    pub fn with_retry_on_context_overflow(mut self, retry_on_context_overflow: bool) -> Self {
        self.retry_on_context_overflow = retry_on_context_overflow;
        self
    }

    pub fn with_active_summary_poll_interval_ms(
        mut self,
        active_summary_poll_interval_ms: u64,
    ) -> Self {
        self.active_summary_poll_interval_ms = active_summary_poll_interval_ms;
        self
    }

    pub fn with_active_summary_poll_timeout_ms(
        mut self,
        active_summary_poll_timeout_ms: u64,
    ) -> Self {
        self.active_summary_poll_timeout_ms = active_summary_poll_timeout_ms;
        self
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    pub fn apply_to_builder(
        &self,
        mut builder: TaskRuntimeBuilder,
    ) -> Result<TaskRuntimeBuilder, String> {
        if self.compose_context {
            builder = builder.with_memory_composer_direct(
                self.base_url.clone(),
                self.timeout(),
                self.source_id.clone(),
            )?;
        }
        if let Some(record_scope) = self.record_scope.clone() {
            let writer = MemoryEngineRecordWriter::new_direct(
                self.base_url.clone(),
                self.timeout(),
                self.source_id.clone(),
                record_scope,
            )?;
            builder = builder.with_record_writer(BestEffortMemoryRecordWriter::new(writer));
        }
        if self.retry_on_context_overflow {
            builder = builder.with_context_overflow_recovery(Some(
                MemoryContextOverflowRecovery::new()
                    .with_trigger_reason("context_overflow")
                    .with_poll_interval(Duration::from_millis(
                        self.active_summary_poll_interval_ms.max(1_000),
                    ))
                    .with_poll_timeout(Duration::from_millis(
                        self.active_summary_poll_timeout_ms.max(10_000),
                    )),
            ));
        }
        Ok(builder)
    }
}

impl TaskRunExecution {
    pub fn new(runtime_config: TaskRuntimeConfig, run_spec: TaskRunSpec) -> Self {
        Self {
            runtime_config,
            run_spec,
        }
    }

    pub fn for_user_text(
        runtime_config: TaskRuntimeConfig,
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        model_config: ModelRuntimeConfig,
        prompt: impl Into<String>,
    ) -> Self {
        Self::new(
            runtime_config,
            TaskRunSpec::new(task_id, run_id, model_config, prompt),
        )
    }

    pub fn with_runtime_config(mut self, runtime_config: TaskRuntimeConfig) -> Self {
        self.runtime_config = runtime_config;
        self
    }

    pub fn with_run_spec(mut self, run_spec: TaskRunSpec) -> Self {
        self.run_spec = run_spec;
        self
    }

    pub fn with_model_config_id(mut self, model_config_id: impl Into<String>) -> Self {
        self.run_spec = self.run_spec.with_model_config_id(model_config_id);
        self
    }

    pub async fn build_runtime(&self) -> Result<TaskRuntime, String> {
        self.runtime_config.build_runtime().await
    }

    pub async fn build_runtime_with_mcp_builder(
        &self,
        mcp_builder: McpExecutorBuilder,
    ) -> Result<TaskRuntime, String> {
        self.runtime_config
            .build_runtime_with_mcp_builder(mcp_builder)
            .await
    }

    pub async fn run_report(&self) -> TaskRunReport {
        match self.build_runtime().await {
            Ok(runtime) => runtime.run_task_report(self.run_spec.clone()).await,
            Err(err) => self.runtime_init_failed_report(err),
        }
    }

    pub async fn run_report_with_mcp_builder(
        &self,
        mcp_builder: McpExecutorBuilder,
    ) -> TaskRunReport {
        match self.build_runtime_with_mcp_builder(mcp_builder).await {
            Ok(runtime) => runtime.run_task_report(self.run_spec.clone()).await,
            Err(err) => self.runtime_init_failed_report(err),
        }
    }

    pub async fn run_report_with_options(
        &self,
        runtime_options: AiRuntimeOptions,
    ) -> TaskRunReport {
        match self.build_runtime().await {
            Ok(runtime) => {
                runtime
                    .run_task_report_with_options(self.run_spec.clone(), runtime_options)
                    .await
            }
            Err(err) => self.runtime_init_failed_report(err),
        }
    }

    pub async fn run_report_with_mcp_builder_and_options(
        &self,
        mcp_builder: McpExecutorBuilder,
        runtime_options: AiRuntimeOptions,
    ) -> TaskRunReport {
        match self.build_runtime_with_mcp_builder(mcp_builder).await {
            Ok(runtime) => {
                runtime
                    .run_task_report_with_options(self.run_spec.clone(), runtime_options)
                    .await
            }
            Err(err) => self.runtime_init_failed_report(err),
        }
    }

    pub async fn run_report_with_runtime(&self, runtime: &TaskRuntime) -> TaskRunReport {
        runtime.run_task_report(self.run_spec.clone()).await
    }

    pub async fn run_report_with_runtime_options(
        &self,
        runtime: &TaskRuntime,
        runtime_options: AiRuntimeOptions,
    ) -> TaskRunReport {
        runtime
            .run_task_report_with_options(self.run_spec.clone(), runtime_options)
            .await
    }

    fn runtime_init_failed_report(&self, err: impl Into<String>) -> TaskRunReport {
        TaskRunReport::from_ai_report(
            self.run_spec.task_id.clone(),
            self.run_spec.run_id.clone(),
            self.run_spec.model_config_id.clone(),
            AiTurnReport::failed(format!("runtime init failed: {}", err.into())),
        )
    }
}

impl Default for TaskRuntimeConfig {
    fn default() -> Self {
        Self {
            http_servers: Vec::new(),
            stdio_servers: Vec::new(),
            builtin_servers: Vec::new(),
            mcp_init_mode: TaskMcpInitMode::default(),
            builtin_prompt_locale: BuiltinMcpPromptLocale::default(),
            builtin_prompt_mode: TaskBuiltinMcpPromptMode::default(),
            max_iterations: None,
            memory_engine: None,
        }
    }
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

impl TaskRunReport {
    pub fn from_ai_report(
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        model_config_id: Option<String>,
        report: AiTurnReport,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            run_id: run_id.into(),
            model_config_id,
            status: report.status,
            content: report.content,
            reasoning: report.reasoning,
            error: report.error,
            tool_calls: report.tool_calls,
            finish_reason: report.finish_reason,
            usage: report.usage,
            response_id: report.response_id,
            completed_at: report.completed_at,
        }
    }

    pub fn is_completed(&self) -> bool {
        self.status == AiTurnStatus::Completed
    }

    pub fn is_aborted(&self) -> bool {
        self.status == AiTurnStatus::Aborted
    }

    pub fn user_message(&self) -> String {
        AiTurnReport {
            status: self.status,
            content: self.content.clone(),
            reasoning: self.reasoning.clone(),
            error: self.error.clone(),
            tool_calls: self.tool_calls.clone(),
            finish_reason: self.finish_reason.clone(),
            usage: self.usage.clone(),
            response_id: self.response_id.clone(),
            completed_at: self.completed_at.clone(),
        }
        .user_message()
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

fn default_memory_timeout_ms() -> u64 {
    30_000
}

fn default_memory_compose_context() -> bool {
    true
}

fn default_retry_on_context_overflow() -> bool {
    true
}

fn default_active_summary_poll_interval_ms() -> u64 {
    10_000
}

fn default_active_summary_poll_timeout_ms() -> u64 {
    120_000
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::{
        TaskBuiltinMcpPromptMode, TaskMcpInitMode, TaskMemoryRuntimeConfig, TaskRunExecution,
        TaskRunSpec, TaskRuntime, TaskRuntimeConfig,
    };
    use crate::{
        AiRuntime, AiTurnStatus, ContextualTurnRunner, MemoryRecordScope, MemoryScope,
        ModelRuntimeConfig, RuntimeRecordOptions,
    };

    #[test]
    fn task_run_spec_serializes_model_config_id_and_builds_turn_request() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        )
        .with_responses_support(true);
        let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
            .with_model_config_id("model_cfg_1")
            .with_memory_scope(Some(MemoryScope::thread(
                "tenant_1",
                "task_runner",
                "task_1",
            )))
            .with_prefixed_input_items(vec![json!({"role":"system","content":"prefix"})])
            .with_tools(vec![json!({"type":"function","name":"tool_1"})]);

        let encoded = serde_json::to_string(&spec).expect("serialize task spec");
        let decoded: TaskRunSpec =
            serde_json::from_str(encoded.as_str()).expect("deserialize task spec");
        let request = decoded.into_contextual_turn_request();

        assert_eq!(request.model_request.model, "gpt-test");
        assert!(request.model_request.supports_responses);
        assert_eq!(request.model_request.tools.len(), 1);
        assert_eq!(
            request.runtime_options.conversation_id.as_deref(),
            Some("task_1")
        );
        assert_eq!(
            request.runtime_options.conversation_turn_id.as_deref(),
            Some("run_1")
        );
        assert_eq!(
            request.runtime_options.caller_model.as_deref(),
            Some("gpt-test")
        );
        assert!(
            request
                .runtime_options
                .record_options
                .persist_assistant_records
        );
        assert!(request.runtime_options.record_options.persist_tool_records);
        assert_eq!(
            request.prefixed_input_items[0]["content"].as_str(),
            Some("prefix")
        );
        assert_eq!(
            request.current_input_items[0]["content"].as_str(),
            Some("execute it")
        );
        assert_eq!(
            request
                .user_record
                .as_ref()
                .and_then(|record| record.metadata.as_ref())
                .and_then(|metadata| metadata.get("model_config_id"))
                .and_then(|value| value.as_str()),
            Some("model_cfg_1")
        );
    }

    #[test]
    fn task_run_spec_injects_configured_builtin_mcp_prompt_from_executor() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
        let executor = chatos_mcp_runtime::McpExecutor::builder()
            .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
            .build();

        let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
            .with_configured_builtin_mcp_prompt_from_executor(
                &executor,
                chatos_mcp_runtime::BuiltinMcpPromptLocale::ZhCn,
            );
        let encoded = serde_json::to_string(&spec).expect("serialize task spec");
        let decoded: TaskRunSpec =
            serde_json::from_str(encoded.as_str()).expect("deserialize task spec");

        assert_eq!(
            decoded
                .builtin_mcp_prompt
                .as_ref()
                .map(|snapshot| &snapshot.mode),
            Some(&TaskBuiltinMcpPromptMode::Configured)
        );
        assert!(decoded
            .prefixed_input_items
            .first()
            .and_then(|item| item.get("content"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|content| content.contains("`task_manager_add_task`")));
    }

    #[test]
    fn task_run_spec_replaces_builtin_mcp_prompt_snapshot() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
        let mut executor = chatos_mcp_runtime::McpExecutor::builder()
            .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
            .build();
        executor.init_builtin_only().expect("builtin init");

        let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
            .with_prefixed_input_items(vec![json!({"role":"system","content":"custom"})])
            .with_configured_builtin_mcp_prompt_from_executor(
                &executor,
                chatos_mcp_runtime::BuiltinMcpPromptLocale::ZhCn,
            )
            .with_effective_builtin_mcp_prompt_from_executor(
                &executor,
                chatos_mcp_runtime::BuiltinMcpPromptLocale::ZhCn,
            );

        assert_eq!(
            spec.builtin_mcp_prompt
                .as_ref()
                .map(|snapshot| &snapshot.mode),
            Some(&TaskBuiltinMcpPromptMode::Effective)
        );
        assert_eq!(spec.prefixed_input_items.len(), 1);
        assert_eq!(
            spec.prefixed_input_items[0]
                .get("content")
                .and_then(serde_json::Value::as_str),
            Some("custom")
        );
    }

    #[test]
    fn task_runtime_builder_prepares_configured_builtin_prompt() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
        let executor = chatos_mcp_runtime::McpExecutor::builder()
            .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
            .build();
        let runtime = TaskRuntime::builder()
            .with_mcp_executor(executor)
            .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured)
            .build();

        let spec = runtime.prepare_spec(TaskRunSpec::new("task_1", "run_1", config, "execute it"));

        assert_eq!(runtime.mcp_executor().map(|_| true), Some(true));
        assert_eq!(
            runtime.builtin_prompt_mode(),
            TaskBuiltinMcpPromptMode::Configured
        );
        assert!(spec
            .prefixed_input_items
            .first()
            .and_then(|item| item.get("content"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|content| content.contains("`task_manager_add_task`")));
    }

    #[test]
    fn task_runtime_builder_defaults_to_effective_builtin_prompt() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
        let executor = chatos_mcp_runtime::McpExecutor::builder()
            .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
            .build_builtin_only()
            .expect("builtin init");
        let runtime = TaskRuntime::builder().with_mcp_executor(executor).build();

        let spec = runtime.prepare_spec(TaskRunSpec::new("task_1", "run_1", config, "execute it"));

        assert_eq!(
            runtime.builtin_prompt_mode(),
            TaskBuiltinMcpPromptMode::Effective
        );
        assert!(spec.prefixed_input_items.is_empty());
        assert_eq!(
            spec.builtin_mcp_prompt
                .as_ref()
                .map(|snapshot| &snapshot.mode),
            Some(&TaskBuiltinMcpPromptMode::Effective)
        );
    }

    #[test]
    fn task_runtime_builder_accepts_builtin_only_mcp_builder() {
        let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new(".");
        let mcp_builder = chatos_mcp_runtime::McpExecutor::builder()
            .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options);

        let runtime = TaskRuntime::builder()
            .with_builtin_only_mcp_executor_builder(mcp_builder)
            .expect("builtin-only mcp executor")
            .build();

        assert!(runtime.mcp_executor().is_some());
        assert!(runtime
            .mcp_executor()
            .expect("executor")
            .unavailable_tools()
            .iter()
            .any(
                |item| item.get("reason").and_then(serde_json::Value::as_str)
                    == Some("missing builtin provider")
            ));
    }

    #[tokio::test]
    async fn task_runtime_builder_accepts_initialized_mcp_builder() {
        let mcp_builder = chatos_mcp_runtime::McpExecutor::builder();

        let runtime = TaskRuntime::builder()
            .with_initialized_mcp_executor_builder(mcp_builder)
            .await
            .expect("initialized mcp executor")
            .build();

        assert!(runtime.mcp_executor().is_some());
        assert!(runtime
            .mcp_executor()
            .expect("executor")
            .available_tools()
            .is_empty());
    }

    #[test]
    fn task_runtime_config_serializes_runtime_shape() {
        let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new("/tmp/task-runtime");
        let config = TaskRuntimeConfig::new()
            .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
            .with_mcp_init_mode(TaskMcpInitMode::BuiltinOnly)
            .with_builtin_prompt_locale(chatos_mcp_runtime::BuiltinMcpPromptLocale::EnUs)
            .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured)
            .with_max_iterations(Some(7));

        let encoded = serde_json::to_string(&config).expect("serialize runtime config");
        let decoded: TaskRuntimeConfig =
            serde_json::from_str(encoded.as_str()).expect("deserialize runtime config");

        assert_eq!(decoded.builtin_servers.len(), 1);
        assert_eq!(decoded.mcp_init_mode, TaskMcpInitMode::BuiltinOnly);
        assert_eq!(
            decoded.builtin_prompt_locale,
            chatos_mcp_runtime::BuiltinMcpPromptLocale::EnUs
        );
        assert_eq!(
            decoded.builtin_prompt_mode,
            TaskBuiltinMcpPromptMode::Configured
        );
        assert_eq!(decoded.max_iterations, Some(7));
        assert_eq!(
            decoded.builtin_servers[0].name,
            chatos_mcp_runtime::TASK_MANAGER_SERVER_NAME
        );
    }

    #[test]
    fn task_memory_runtime_config_serializes_direct_memory_settings() {
        let memory = TaskMemoryRuntimeConfig::new("http://127.0.0.1:1", "task_runner")
            .with_timeout_ms(500)
            .with_compose_context(false)
            .with_record_scope(Some(MemoryRecordScope::message_thread(
                "tenant_1", "thread_1",
            )));
        let config = TaskRuntimeConfig::new().with_memory_engine(Some(memory));

        let encoded = serde_json::to_string(&config).expect("serialize memory runtime config");
        let decoded: TaskRuntimeConfig =
            serde_json::from_str(encoded.as_str()).expect("deserialize memory runtime config");
        let memory = decoded.memory_engine.expect("memory config");

        assert_eq!(memory.base_url, "http://127.0.0.1:1");
        assert_eq!(memory.source_id, "task_runner");
        assert_eq!(memory.timeout_ms, 500);
        assert!(!memory.compose_context);
        assert!(memory.retry_on_context_overflow);
        assert_eq!(memory.active_summary_poll_interval_ms, 10_000);
        assert_eq!(memory.active_summary_poll_timeout_ms, 120_000);
        assert_eq!(
            memory
                .record_scope
                .as_ref()
                .map(|scope| scope.record_type.as_str()),
            Some("message")
        );
    }

    #[tokio::test]
    async fn task_runtime_config_builds_builtin_only_runtime() {
        let options = chatos_mcp_runtime::BuiltinMcpServerOptions::new("/tmp/task-runtime");
        let config = TaskRuntimeConfig::new()
            .with_builtin_kinds([chatos_mcp_runtime::BuiltinMcpKind::TaskManager], &options)
            .with_mcp_init_mode(TaskMcpInitMode::BuiltinOnly)
            .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Effective);

        let runtime = config.build_runtime().await.expect("runtime");

        assert!(runtime.mcp_executor().is_some());
        assert!(runtime
            .mcp_executor()
            .expect("executor")
            .unavailable_tools()
            .iter()
            .any(
                |item| item.get("reason").and_then(serde_json::Value::as_str)
                    == Some("missing builtin provider")
            ));
    }

    #[tokio::test]
    async fn task_runtime_config_can_disable_mcp() {
        let config = TaskRuntimeConfig::new()
            .with_mcp_init_mode(TaskMcpInitMode::Disabled)
            .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured);

        let runtime = config.build_runtime().await.expect("runtime");

        assert!(runtime.mcp_executor().is_none());
        assert_eq!(
            runtime.builtin_prompt_mode(),
            TaskBuiltinMcpPromptMode::Configured
        );
    }

    #[tokio::test]
    async fn task_runtime_config_builds_with_memory_engine_config() {
        let memory = TaskMemoryRuntimeConfig::new("http://127.0.0.1:1", "task_runner")
            .with_timeout_ms(100)
            .with_record_scope(Some(MemoryRecordScope::message_thread(
                "tenant_1", "thread_1",
            )));
        let config = TaskRuntimeConfig::new()
            .with_mcp_init_mode(TaskMcpInitMode::Disabled)
            .with_memory_engine(Some(memory));

        let runtime = config.build_runtime().await.expect("runtime");

        assert!(runtime.mcp_executor().is_none());
    }

    #[test]
    fn task_run_execution_serializes_runtime_config_and_spec() {
        let runtime_config = TaskRuntimeConfig::new()
            .with_mcp_init_mode(TaskMcpInitMode::Disabled)
            .with_builtin_prompt_mode(TaskBuiltinMcpPromptMode::Configured);
        let model_config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let execution =
            TaskRunExecution::for_user_text(runtime_config, "task_1", "run_1", model_config, "go")
                .with_model_config_id("model_cfg_1");

        let encoded = serde_json::to_string(&execution).expect("serialize execution");
        let decoded: TaskRunExecution =
            serde_json::from_str(encoded.as_str()).expect("deserialize execution");

        assert_eq!(
            decoded.runtime_config.mcp_init_mode,
            TaskMcpInitMode::Disabled
        );
        assert_eq!(decoded.run_spec.task_id, "task_1");
        assert_eq!(decoded.run_spec.run_id, "run_1");
        assert_eq!(
            decoded.run_spec.model_config_id.as_deref(),
            Some("model_cfg_1")
        );
    }

    #[tokio::test]
    async fn task_run_execution_runs_with_runtime_options() {
        let runtime_config = TaskRuntimeConfig::new().with_mcp_init_mode(TaskMcpInitMode::Disabled);
        let model_config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:1/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let execution =
            TaskRunExecution::for_user_text(runtime_config, "task_1", "run_1", model_config, "go");
        let options = execution
            .run_spec
            .runtime_options()
            .with_abort_checker(Some(Arc::new(|conversation_id| {
                conversation_id == "task_1"
            })));

        let report = execution.run_report_with_options(options).await;

        assert_eq!(report.status, AiTurnStatus::Aborted);
        assert_eq!(report.task_id, "task_1");
        assert_eq!(report.run_id, "run_1");
    }

    #[test]
    fn task_run_execution_wraps_runtime_init_failure_report() {
        let runtime_config = TaskRuntimeConfig::new().with_mcp_init_mode(TaskMcpInitMode::Full);
        let model_config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:1/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let execution =
            TaskRunExecution::for_user_text(runtime_config, "task_1", "run_1", model_config, "go");

        let report = execution.runtime_init_failed_report("boom");

        assert_eq!(report.status, AiTurnStatus::Failed);
        assert!(report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("runtime init failed")));
    }

    #[tokio::test]
    async fn task_runner_report_captures_aborted_runtime() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:1/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let spec = TaskRunSpec::new("task_1", "run_1", config, "execute it")
            .with_model_config_id("model_cfg_1")
            .with_record_options(RuntimeRecordOptions::default());
        let options =
            spec.runtime_options()
                .with_abort_checker(Some(Arc::new(|conversation_id| {
                    conversation_id == "task_1"
                })));
        let runner = ContextualTurnRunner::new(AiRuntime::new(None), None);

        let report = runner.run_task_report_with_options(spec, options).await;

        assert_eq!(report.task_id, "task_1");
        assert_eq!(report.run_id, "run_1");
        assert_eq!(report.model_config_id.as_deref(), Some("model_cfg_1"));
        assert_eq!(report.status, AiTurnStatus::Aborted);
        assert!(report.is_aborted());
        assert_eq!(report.user_message(), "任务已取消。");
    }
}

use serde::{Deserialize, Serialize};

use chatos_mcp_runtime::{
    builtin_servers_from_kinds, BuiltinMcpKind, BuiltinMcpPromptLocale, BuiltinMcpServerOptions,
    McpBuiltinServer, McpExecutorBuilder, McpHttpServer, McpStdioServer,
};

use super::memory::TaskMemoryRuntimeConfig;
use super::runtime_builder::TaskRuntimeBuilder;
use super::{TaskBuiltinMcpPromptMode, TaskRuntime};

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

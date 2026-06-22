use serde::{Deserialize, Serialize};

use chatos_mcp_runtime::McpExecutorBuilder;

use crate::runtime::{AiRuntimeOptions, AiTurnReport};
use crate::traits::ModelRuntimeConfig;

use super::{TaskRunReport, TaskRunSpec, TaskRuntime, TaskRuntimeConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunExecution {
    pub runtime_config: TaskRuntimeConfig,
    pub run_spec: TaskRunSpec,
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

    pub(crate) fn runtime_init_failed_report(&self, err: impl Into<String>) -> TaskRunReport {
        TaskRunReport::from_ai_report(
            self.run_spec.task_id.clone(),
            self.run_spec.run_id.clone(),
            self.run_spec.model_config_id.clone(),
            AiTurnReport::failed(format!("runtime init failed: {}", err.into())),
        )
    }
}

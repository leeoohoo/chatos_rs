// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{
    TaskRunnerCapabilitySnapshot, TaskRunnerExecutionTool, TaskRunnerProjectSnapshot,
    TaskRunnerPromptSnapshot,
};
use chatos_client_storage::{
    ClientSettingRecord, ClientStorage, ProjectRecord, RecordQuery, RecordScope, StorageError,
    StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::FrozenSnapshot;
use serde::Deserialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    host::stable_host_id, LocalTaskCreationPlan, LocalTaskCreationPlanner, LocalTaskPlanningRequest,
};

pub const TASK_RUNNER_PROMPT_SETTING_ID: &str = "local-agent-task-runner-prompt";
pub const TASK_RUNNER_PROMPT_SETTING_KEY: &str = "local_agent.task_runner_prompt";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTaskProjectConfiguration {
    pub schema_version: u32,
    pub task_model_config_id: String,
    pub authority_snapshot: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTaskPromptConfiguration {
    pub schema_version: u32,
    pub prompt_revision: String,
    pub base_system_prompt: String,
    pub task_prompt: String,
    pub skill_snapshot: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalTaskCapabilityResolution {
    pub resolution_revision: String,
    pub plugin_release_snapshot: Value,
    pub execution_tools: Vec<TaskRunnerExecutionTool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalTaskCapabilityRequest {
    pub task_id: String,
    pub owner_user_id: String,
    pub project_snapshot: TaskRunnerProjectSnapshot,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub parent_capability_snapshot_ref: String,
}

#[async_trait]
pub trait LocalTaskCapabilityResolver: Send + Sync {
    async fn resolve_capabilities(
        &self,
        request: &LocalTaskCapabilityRequest,
        cancellation: CancellationToken,
    ) -> Result<LocalTaskCapabilityResolution, String>;
}

pub struct StoredLocalTaskCreationPlanner {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    capability_resolver: Arc<dyn LocalTaskCapabilityResolver>,
}

impl StoredLocalTaskCreationPlanner {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        capability_resolver: Arc<dyn LocalTaskCapabilityResolver>,
    ) -> Self {
        Self {
            storage,
            scope,
            capability_resolver,
        }
    }

    async fn load_sources(&self, project_id: &str) -> StorageResult<LocalTaskPlanningSources> {
        let mut operation = LoadLocalTaskPlanningSources {
            scope: self.scope.clone(),
            project_id: project_id.to_string(),
            result: None,
        };
        self.storage.transaction(&mut operation).await?;
        operation.result.ok_or(StorageError::Transaction {
            reason: "local Task planning source query returned no result".to_string(),
        })
    }
}

#[async_trait]
impl LocalTaskCreationPlanner for StoredLocalTaskCreationPlanner {
    async fn plan_task(
        &self,
        request: &LocalTaskPlanningRequest,
        cancellation: CancellationToken,
    ) -> Result<LocalTaskCreationPlan, String> {
        if cancellation.is_cancelled() {
            return Err("local Task planning was cancelled".to_string());
        }
        if request.project_id.trim().is_empty()
            || request.task_id.trim().is_empty()
            || request.objective.trim().is_empty()
            || request.acceptance_criteria.is_empty()
        {
            return Err("local Task planning request is incomplete".to_string());
        }
        let sources = self
            .load_sources(&request.project_id)
            .await
            .map_err(|error| format!("failed to load local Task planning sources: {error}"))?;
        if sources.project.metadata.scope != self.scope
            || sources.project.metadata.id != request.project_id
            || sources.prompt.metadata.scope != self.scope
            || sources.prompt.metadata.id != TASK_RUNNER_PROMPT_SETTING_ID
            || sources.prompt.key != TASK_RUNNER_PROMPT_SETTING_KEY
        {
            return Err(
                "local Task planning sources are outside the frozen owner scope".to_string(),
            );
        }
        let project_config: LocalTaskProjectConfiguration =
            serde_json::from_value(sources.project.state.clone())
                .map_err(|error| format!("invalid local Task project configuration: {error}"))?;
        validate_project_configuration(&project_config)?;
        let prompt_config: LocalTaskPromptConfiguration =
            serde_json::from_value(sources.prompt.value.clone())
                .map_err(|error| format!("invalid Task Runner prompt configuration: {error}"))?;
        validate_prompt_configuration(&prompt_config)?;
        let working_directory_ref =
            sources.project.root_reference.as_deref().ok_or_else(|| {
                "local Task project has no working directory reference".to_string()
            })?;
        validate_opaque_reference(working_directory_ref)?;
        let project_revision = format!("project-record-{}", sources.project.metadata.revision);
        let project = TaskRunnerProjectSnapshot {
            project_id: request.project_id.clone(),
            snapshot_revision: project_revision.clone(),
            working_directory_ref: working_directory_ref.to_string(),
            authority_snapshot: project_config.authority_snapshot,
        };
        project.validate()?;
        let capability_request = LocalTaskCapabilityRequest {
            task_id: request.task_id.clone(),
            owner_user_id: self.scope.owner_user_id.clone(),
            project_snapshot: project.clone(),
            objective: request.objective.clone(),
            acceptance_criteria: request.acceptance_criteria.clone(),
            parent_capability_snapshot_ref: request.parent_capability_snapshot_ref.clone(),
        };
        let resolution = self
            .capability_resolver
            .resolve_capabilities(&capability_request, cancellation.clone())
            .await?;
        if cancellation.is_cancelled() {
            return Err("local Task planning was cancelled".to_string());
        }
        if resolution.resolution_revision.trim().is_empty() {
            return Err("local capability resolution has no revision".to_string());
        }
        let capability_id = stable_host_id(
            "task-capability-snapshot",
            &[
                self.scope.owner_user_id.as_str(),
                request.task_id.as_str(),
                request.project_id.as_str(),
                project_revision.as_str(),
                resolution.resolution_revision.as_str(),
            ],
        );
        let capability = TaskRunnerCapabilitySnapshot {
            snapshot_ref: capability_id.clone(),
            plugin_release_snapshot: resolution.plugin_release_snapshot,
            execution_tools: resolution.execution_tools,
        };
        capability.validate()?;
        let prompt = TaskRunnerPromptSnapshot {
            prompt_revision: prompt_config.prompt_revision.clone(),
            base_system_prompt: prompt_config.base_system_prompt,
            task_prompt: prompt_config.task_prompt,
            skill_snapshot: prompt_config.skill_snapshot,
        };
        prompt.validate()?;
        let prompt_id = stable_host_id(
            "task-prompt-snapshot",
            &[
                self.scope.owner_user_id.as_str(),
                sources.prompt.metadata.id.as_str(),
                prompt_config.prompt_revision.as_str(),
                sources.prompt.metadata.revision.to_string().as_str(),
            ],
        );
        let project_id = stable_host_id(
            "task-project-snapshot",
            &[
                self.scope.owner_user_id.as_str(),
                request.project_id.as_str(),
                project_revision.as_str(),
            ],
        );
        Ok(LocalTaskCreationPlan {
            project_id: request.project_id.clone(),
            model_config_id: project_config.task_model_config_id,
            prompt_snapshot: FrozenSnapshot::new(
                prompt_id,
                prompt_config.prompt_revision,
                serde_json::to_value(prompt)
                    .map_err(|error| format!("failed to freeze Task Runner prompt: {error}"))?,
            )
            .map_err(|error| format!("invalid frozen Task Runner prompt: {error}"))?,
            project_snapshot: FrozenSnapshot::new(
                project_id,
                project_revision,
                serde_json::to_value(project)
                    .map_err(|error| format!("failed to freeze Task project: {error}"))?,
            )
            .map_err(|error| format!("invalid frozen Task project: {error}"))?,
            capability_snapshot: FrozenSnapshot::new(
                capability_id,
                resolution.resolution_revision,
                serde_json::to_value(capability)
                    .map_err(|error| format!("failed to freeze Task capabilities: {error}"))?,
            )
            .map_err(|error| format!("invalid frozen Task capabilities: {error}"))?,
        })
    }
}

struct LocalTaskPlanningSources {
    project: ProjectRecord,
    prompt: ClientSettingRecord,
}

struct LoadLocalTaskPlanningSources {
    scope: RecordScope,
    project_id: String,
    result: Option<LocalTaskPlanningSources>,
}

#[async_trait]
impl StorageTransaction for LoadLocalTaskPlanningSources {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let project = repositories
            .projects()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: self.project_id.clone(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        let prompt = repositories
            .settings()
            .get(&RecordQuery {
                scope: self.scope.clone(),
                id: TASK_RUNNER_PROMPT_SETTING_ID.to_string(),
            })
            .await?
            .ok_or(StorageError::NotFound)?;
        self.result = Some(LocalTaskPlanningSources { project, prompt });
        Ok(())
    }
}

fn validate_project_configuration(config: &LocalTaskProjectConfiguration) -> Result<(), String> {
    if config.schema_version != 1 {
        return Err("unsupported local Task project configuration version".to_string());
    }
    if config.task_model_config_id.trim().is_empty() {
        return Err("local Task project has no model configuration".to_string());
    }
    if !config.authority_snapshot.is_object() {
        return Err("local Task project authority snapshot must be an object".to_string());
    }
    Ok(())
}

fn validate_prompt_configuration(config: &LocalTaskPromptConfiguration) -> Result<(), String> {
    if config.schema_version != 1 {
        return Err("unsupported Task Runner prompt configuration version".to_string());
    }
    let prompt = TaskRunnerPromptSnapshot {
        prompt_revision: config.prompt_revision.clone(),
        base_system_prompt: config.base_system_prompt.clone(),
        task_prompt: config.task_prompt.clone(),
        skill_snapshot: config.skill_snapshot.clone(),
    };
    prompt.validate()
}

fn validate_opaque_reference(value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > 512
        || value.starts_with('/')
        || value.contains(['/', '\\'])
        || value.chars().any(char::is_control)
    {
        return Err("working directory reference must be an opaque local grant ID".to_string());
    }
    Ok(())
}

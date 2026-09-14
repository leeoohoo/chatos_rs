// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{
    TaskRunnerCapabilitySnapshot, TaskRunnerExecutionTool, TaskRunnerProjectSnapshot,
    TaskRunnerPromptSnapshot,
};
use chatos_client_storage::{
    ClientSettingRecord, ClientStorage, ProjectRecord, PutRecord, RecordMetadata, RecordQuery,
    RecordScope, StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::FrozenSnapshot;
use chatos_memory_client::RotatingBearerToken;
use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, validate_agent_prompt_checksum, ResolveAgentPromptRequest,
    ResolvedAgentPrompt, SystemAgentKey, DEFAULT_AGENT_PROMPT_PROFILE,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    host::stable_host_id, LocalTaskCreationPlan, LocalTaskCreationPlanner, LocalTaskPlanningRequest,
};

pub const TASK_RUNNER_PROMPT_SETTING_KEY: &str = "local_agent.task_runner_prompt";
pub const TASK_RUNNER_PROMPT_SETTING_ID: &str = "client_setting:local_agent.task_runner_prompt";

#[async_trait]
pub trait LocalTaskPromptSource: Send + Sync {
    async fn resolve_prompt(
        &self,
        model_provider: &str,
        cancellation: CancellationToken,
    ) -> Result<LocalTaskPromptConfiguration, String>;
}

pub struct PluginManagementTaskPromptSource {
    base_url: String,
    access_token: RotatingBearerToken,
    http: reqwest::Client,
}

impl PluginManagementTaskPromptSource {
    pub fn new(
        base_url: impl Into<String>,
        access_token: RotatingBearerToken,
    ) -> Result<Self, String> {
        let base_url = base_url.into().trim().trim_end_matches('/').to_string();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Plugin Management URL is invalid: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err("Plugin Management URL must be an absolute HTTP(S) URL".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| format!("Plugin Management client could not start: {error}"))?;
        Ok(Self {
            base_url,
            access_token,
            http,
        })
    }
}

#[async_trait]
impl LocalTaskPromptSource for PluginManagementTaskPromptSource {
    async fn resolve_prompt(
        &self,
        model_provider: &str,
        cancellation: CancellationToken,
    ) -> Result<LocalTaskPromptConfiguration, String> {
        let vendor = required_agent_prompt_vendor(None, model_provider)
            .map_err(|error| error.to_string())?;
        let access_token = self
            .access_token
            .snapshot()
            .map_err(|_| "model access token is unavailable".to_string())?;
        let request = self
            .http
            .post(format!("{}/runtime/agent-prompts/resolve", self.base_url))
            .bearer_auth(access_token.as_str())
            .json(&ResolveAgentPromptRequest {
                agent_key: SystemAgentKey::TaskRunnerRunPhase,
                vendor,
                profile: Some(DEFAULT_AGENT_PROMPT_PROFILE.to_string()),
            })
            .send();
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err("Task Runner Prompt sync was cancelled".to_string()),
            response = request => response.map_err(|error| format!("Task Runner Prompt request failed: {error}"))?,
        };
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(format!(
                "Task Runner Prompt request was rejected with HTTP {}: {}",
                status.as_u16(),
                detail.chars().take(1_024).collect::<String>()
            ));
        }
        let resolved: ResolvedAgentPrompt = response
            .json()
            .await
            .map_err(|error| format!("Task Runner Prompt response is invalid: {error}"))?;
        if resolved.agent_key != SystemAgentKey::TaskRunnerRunPhase.as_str()
            || resolved.vendor != vendor
            || resolved.revision <= 0
            || !validate_agent_prompt_checksum(
                resolved.content.as_str(),
                resolved.checksum.as_str(),
            )
        {
            return Err(
                "Task Runner Prompt response failed identity or checksum validation".to_string(),
            );
        }
        Ok(LocalTaskPromptConfiguration {
            schema_version: 1,
            prompt_revision: format!(
                "{}:{}:{}",
                resolved.vendor, resolved.revision, resolved.checksum
            ),
            base_system_prompt: resolved.content,
            skill_snapshot: serde_json::json!({
                "source": "plugin_management",
                "vendor": resolved.vendor,
                "published_at": resolved.published_at,
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTaskPromptConfiguration {
    pub schema_version: u32,
    pub prompt_revision: String,
    pub base_system_prompt: String,
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
    origin_device_id: String,
    prompt_source: Arc<dyn LocalTaskPromptSource>,
    capability_resolver: Arc<dyn LocalTaskCapabilityResolver>,
}

impl StoredLocalTaskCreationPlanner {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        origin_device_id: impl Into<String>,
        prompt_source: Arc<dyn LocalTaskPromptSource>,
        capability_resolver: Arc<dyn LocalTaskCapabilityResolver>,
    ) -> Self {
        Self {
            storage,
            scope,
            origin_device_id: origin_device_id.into(),
            prompt_source,
            capability_resolver,
        }
    }

    async fn sync_prompt(
        &self,
        model_provider: &str,
        cancellation: CancellationToken,
    ) -> Result<(), String> {
        let prompt = self
            .prompt_source
            .resolve_prompt(model_provider, cancellation)
            .await?;
        validate_prompt_configuration(&prompt)?;
        let mut operation = PersistTaskRunnerPrompt {
            scope: self.scope.clone(),
            origin_device_id: self.origin_device_id.clone(),
            prompt: Some(prompt),
            now: chrono::Utc::now(),
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(|error| format!("failed to persist resolved Task Runner Prompt: {error}"))
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
            || request.model_config_id.trim().is_empty()
            || request.model_provider.trim().is_empty()
            || request.objective.trim().is_empty()
            || request.acceptance_criteria.is_empty()
        {
            return Err("local Task planning request is incomplete".to_string());
        }
        self.sync_prompt(&request.model_provider, cancellation.clone())
            .await?;
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
        let project = crate::project_snapshot(sources.project.clone())
            .map_err(|error| format!("invalid local Task project configuration: {error}"))?;
        if project.status != chatos_local_agent_protocol::LocalProjectStatus::Active
            || project.project_id != request.project_id
            || project.owner_user_id != self.scope.owner_user_id
        {
            return Err("local Task project is inactive or outside the frozen scope".to_string());
        }
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
            authority_snapshot: serde_json::json!({
                "schema_version": 1,
                "root_path": project.draft.root_path,
            }),
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
            model_config_id: request.model_config_id.clone(),
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

struct PersistTaskRunnerPrompt {
    scope: RecordScope,
    origin_device_id: String,
    prompt: Option<LocalTaskPromptConfiguration>,
    now: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
impl StorageTransaction for PersistTaskRunnerPrompt {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let prompt = self.prompt.take().ok_or(StorageError::Transaction {
            reason: "Task Runner Prompt persistence request was already consumed".to_string(),
        })?;
        let repository = &mut *repositories.settings();
        let query = RecordQuery {
            scope: self.scope.clone(),
            id: TASK_RUNNER_PROMPT_SETTING_ID.to_string(),
        };
        let current = repository.get(&query).await?;
        if let Some(current) = current.as_ref() {
            if current.key != TASK_RUNNER_PROMPT_SETTING_KEY {
                return Err(StorageError::InvalidData {
                    reason: "Task Runner Prompt setting identity does not match its key"
                        .to_string(),
                });
            }
            let existing: LocalTaskPromptConfiguration =
                serde_json::from_value(current.value.clone()).map_err(|error| {
                    StorageError::InvalidData {
                        reason: format!("persisted Task Runner Prompt is invalid: {error}"),
                    }
                })?;
            if existing == prompt {
                return Ok(());
            }
        }
        let expected_revision = current.as_ref().map(|record| record.metadata.revision);
        let metadata = current
            .map(|record| record.metadata)
            .unwrap_or_else(|| RecordMetadata {
                id: TASK_RUNNER_PROMPT_SETTING_ID.to_string(),
                scope: self.scope.clone(),
                origin_device_id: self.origin_device_id.clone(),
                revision: 0,
                created_at: self.now,
                updated_at: self.now,
            });
        repository
            .put(PutRecord {
                record: ClientSettingRecord {
                    metadata,
                    key: TASK_RUNNER_PROMPT_SETTING_KEY.to_string(),
                    value: serde_json::to_value(prompt).map_err(|error| {
                        StorageError::InvalidData {
                            reason: format!(
                                "resolved Task Runner Prompt cannot be stored: {error}"
                            ),
                        }
                    })?,
                },
                expected_revision,
            })
            .await?;
        Ok(())
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

fn validate_prompt_configuration(config: &LocalTaskPromptConfiguration) -> Result<(), String> {
    if config.schema_version != 1 {
        return Err("unsupported Task Runner prompt configuration version".to_string());
    }
    let prompt = TaskRunnerPromptSnapshot {
        prompt_revision: config.prompt_revision.clone(),
        base_system_prompt: config.base_system_prompt.clone(),
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

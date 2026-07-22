// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chatos_mcp::{system_mcp_descriptor_for_record, SystemMcpHost};
use chatos_mcp_runtime::{
    builtin_kind_by_any, BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback,
};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::local_runtime::storage::LocalDatabase;
use crate::local_runtime::task_board::LocalTaskBoardTaskRecord;
use crate::local_runtime::task_runner::CreateLocalConversationTaskInput;
use crate::mcp::manifest::LocalMcpManifestRecord;
use crate::LocalState;

const SERVER_NAME: &str = "task_runner_service";

#[derive(Clone)]
pub(crate) struct LocalTaskRunnerServiceProvider {
    database: LocalDatabase,
    owner_user_id: String,
    project_id: String,
    session_id: String,
    source_turn_id: String,
    default_model_config_id: Option<String>,
    models: Vec<Value>,
    selectable_builtin_kinds: Vec<String>,
    planning_builtin_kinds: Vec<String>,
    external_mcp_configs: Vec<Value>,
    available_skills: Vec<Value>,
    execution_external_mcp_ids: BTreeSet<String>,
    planning_external_mcp_ids: BTreeSet<String>,
    execution_skill_ids: BTreeSet<String>,
    planning_skill_ids: BTreeSet<String>,
}

impl LocalTaskRunnerServiceProvider {
    pub(crate) async fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        project_id: impl Into<String>,
        session_id: impl Into<String>,
        source_turn_id: impl Into<String>,
        default_model_config_id: Option<String>,
        state: &LocalState,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into();
        let capabilities = database
            .get_capability_snapshot(
                owner_user_id.as_str(),
                SystemAgentKey::TaskRunnerRunPhase.as_str(),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "Plugin capability snapshot is missing for task_runner_run_phase".to_string()
            })?;
        capabilities
            .ensure_required_available()
            .map_err(|error| error.to_string())?;
        let planning_capabilities = database
            .get_capability_snapshot(
                owner_user_id.as_str(),
                SystemAgentKey::TaskRunnerPlanPhase.as_str(),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "Plugin capability snapshot is missing for task_runner_plan_phase".to_string()
            })?;
        planning_capabilities
            .ensure_required_available()
            .map_err(|error| error.to_string())?;
        let manifests = database
            .list_mcp_manifests(
                owner_user_id.as_str(),
                state.device_id.as_deref().unwrap_or_default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let selectable_builtin_kinds = capabilities
            .selectable_mcps()
            .filter_map(|item| system_mcp_descriptor_for_record(&item.resource))
            .filter(|descriptor| descriptor.supports_host(SystemMcpHost::LocalConnector))
            .filter_map(|descriptor| descriptor.embedded_kind)
            .map(|kind| kind.kind_name().to_string())
            .collect::<Vec<_>>();
        let planning_builtin_kinds = planning_capabilities
            .selectable_mcps()
            .filter_map(|item| system_mcp_descriptor_for_record(&item.resource))
            .filter(|descriptor| descriptor.supports_host(SystemMcpHost::LocalConnector))
            .filter_map(|descriptor| descriptor.embedded_kind)
            .map(|kind| kind.kind_name().to_string())
            .collect::<Vec<_>>();
        let execution_external_mcp_configs =
            local_external_mcp_views(&capabilities, manifests.as_slice());
        let planning_external_mcp_configs =
            local_external_mcp_views(&planning_capabilities, manifests.as_slice());
        let execution_available_skills = capabilities
            .selectable_skills()
            .map(|item| {
                json!({
                    "id": item.resource.id,
                    "name": item.resource.name,
                    "display_name": item.resource.display_name,
                    "description": item.resource.description,
                    "status": item.status,
                })
            })
            .collect::<Vec<_>>();
        let planning_available_skills = planning_capabilities
            .selectable_skills()
            .map(|item| {
                json!({
                    "id": item.resource.id,
                    "name": item.resource.name,
                    "display_name": item.resource.display_name,
                    "description": item.resource.description,
                    "status": item.status,
                })
            })
            .collect::<Vec<_>>();
        let execution_external_mcp_ids = catalog_ids(execution_external_mcp_configs.as_slice());
        let planning_external_mcp_ids = catalog_ids(planning_external_mcp_configs.as_slice());
        let execution_skill_ids = catalog_ids(execution_available_skills.as_slice());
        let planning_skill_ids = catalog_ids(planning_available_skills.as_slice());
        let external_mcp_configs = merge_catalogs(
            execution_external_mcp_configs,
            planning_external_mcp_configs,
        );
        let available_skills =
            merge_catalogs(execution_available_skills, planning_available_skills);
        let models = state
            .model_configs
            .configs
            .iter()
            .filter(|model| {
                model.enabled
                    && model
                        .api_key
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|value| !value.is_empty())
            })
            .map(|model| {
                json!({
                    "id": model.id,
                    "name": model.name,
                    "model": model.model,
                    "usage_scenario": model.task_usage_scenario,
                })
            })
            .collect::<Vec<_>>();
        Ok(Self {
            database,
            owner_user_id,
            project_id: project_id.into(),
            session_id: session_id.into(),
            source_turn_id: source_turn_id.into(),
            default_model_config_id,
            models,
            selectable_builtin_kinds,
            planning_builtin_kinds,
            external_mcp_configs,
            available_skills,
            execution_external_mcp_ids,
            planning_external_mcp_ids,
            execution_skill_ids,
            planning_skill_ids,
        })
    }

    fn tools(&self) -> Vec<Value> {
        let task_properties = self.task_properties();
        vec![
            tool(
                "list_tasks",
                "查看当前联系人会话中由本地任务系统创建的历史任务。",
                json!({
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"},
                        "keyword": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                        "offset": {"type": "integer", "minimum": 0}
                    },
                    "additionalProperties": false
                }),
            ),
            tool(
                "get_task",
                "读取一个本地任务的状态、目标和执行结果。",
                required_id_schema("task_id"),
            ),
            tool(
                "create_task",
                "为当前联系人消息创建一个由客户端本地后台执行的任务。",
                json!({
                    "type": "object",
                    "properties": task_properties,
                    "required": ["title", "objective", "is_planning_task"],
                    "additionalProperties": false
                }),
            ),
            tool(
                "list_mcp_builtin_catalog",
                "列出插件管理允许本地任务选择的内置能力。",
                empty_schema(),
            ),
            tool(
                "list_external_mcp_configs",
                "列出插件管理允许且当前设备可执行的外部 MCP。",
                empty_schema(),
            ),
            tool(
                "list_available_skills",
                "列出插件管理允许且当前设备可用的 Skills。",
                empty_schema(),
            ),
            tool(
                "create_tasks_with_prerequisites",
                "一次创建多个本地后台任务，并用 client_ref 声明前置关系。",
                json!({
                    "type": "object",
                    "properties": {
                        "tasks": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 50,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "client_ref": {"type": "string", "minLength": 1},
                                    "prerequisite_refs": {"type": "array", "items": {"type": "string"}, "uniqueItems": true},
                                    "prerequisite_task_ids": {"type": "array", "items": {"type": "string"}, "uniqueItems": true},
                                    "title": task_properties["title"].clone(),
                                    "description": task_properties["description"].clone(),
                                    "objective": task_properties["objective"].clone(),
                                    "priority": task_properties["priority"].clone(),
                                    "tags": task_properties["tags"].clone(),
                                    "default_model_config_id": task_properties["default_model_config_id"].clone(),
                                    "is_planning_task": task_properties["is_planning_task"].clone(),
                                    "enabled_builtin_kinds": task_properties["enabled_builtin_kinds"].clone(),
                                    "external_mcp_config_ids": task_properties["external_mcp_config_ids"].clone(),
                                    "selected_skill_ids": task_properties["selected_skill_ids"].clone()
                                },
                                "required": ["client_ref", "title", "objective", "is_planning_task"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["tasks"],
                    "additionalProperties": false
                }),
            ),
            tool(
                "cancel_task",
                "取消当前联系人会话中的本地任务。",
                json!({
                    "type": "object",
                    "properties": {
                        "task_id": {"type": "string", "minLength": 1},
                        "reason": {"type": "string", "minLength": 1},
                        "replacement_task_ids": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["task_id", "reason"],
                    "additionalProperties": false
                }),
            ),
            tool(
                "wait_for_task_completion",
                "确认已创建的任务继续在客户端本地后台执行。",
                empty_schema(),
            ),
            tool(
                "get_task_dependency_graph",
                "读取一个本地任务的前置依赖关系。",
                required_id_schema("task_id"),
            ),
        ]
    }

    fn task_properties(&self) -> Value {
        let builtin_kinds = self
            .selectable_builtin_kinds
            .iter()
            .chain(self.planning_builtin_kinds.iter())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let model_ids = self
            .models
            .iter()
            .filter_map(|item| item.get("id").cloned())
            .collect::<Vec<_>>();
        let external_ids = self
            .external_mcp_configs
            .iter()
            .filter_map(|item| item.get("id").cloned())
            .collect::<Vec<_>>();
        let skill_ids = self
            .available_skills
            .iter()
            .filter_map(|item| item.get("id").cloned())
            .collect::<Vec<_>>();
        json!({
            "title": {"type": "string", "minLength": 1, "description": "任务标题。"},
            "description": {"type": "string", "description": "任务背景和上下文。"},
            "objective": {"type": "string", "minLength": 1, "description": "可验证的具体执行目标。"},
            "priority": {"type": "integer", "description": "数值越高优先级越高。"},
            "tags": {"type": "array", "items": {"type": "string"}},
            "default_model_config_id": {
                "type": "string",
                "enum": model_ids,
                "description": "当前设备上可用的本地模型配置；省略时使用当前会话模型。"
            },
            "is_planning_task": {
                "type": "boolean",
                "description": "规划、拆解类任务为 true；实际执行、检索、修改类任务为 false。"
            },
            "enabled_builtin_kinds": {
                "type": "array",
                "items": {"type": "string", "enum": builtin_kinds},
                "uniqueItems": true,
                "description": "只能选择插件管理对本地 Task Runner Run Phase 开放的内置能力。需要写入能力时必须同时选择读取能力。"
            },
            "external_mcp_config_ids": {
                "type": "array",
                "items": {"type": "string", "enum": external_ids},
                "uniqueItems": true
            },
            "selected_skill_ids": {
                "type": "array",
                "items": {"type": "string", "enum": skill_ids},
                "uniqueItems": true
            }
        })
    }

    fn source(&self, context: &ToolCallContext) -> Result<(String, String), String> {
        let session_id = context
            .conversation_id
            .as_deref()
            .unwrap_or(self.session_id.as_str())
            .trim();
        let turn_id = context
            .conversation_turn_id
            .as_deref()
            .unwrap_or(self.source_turn_id.as_str())
            .trim();
        if session_id != self.session_id || turn_id != self.source_turn_id {
            return Err(
                "Local Task Runner source context does not match the active local turn".to_string(),
            );
        }
        Ok((session_id.to_string(), turn_id.to_string()))
    }

    async fn create_one(
        &self,
        args: CreateTaskArgs,
        context: &ToolCallContext,
        prerequisite_task_ids: Vec<String>,
    ) -> Result<LocalTaskBoardTaskRecord, String> {
        let (session_id, source_turn_id) = self.source(context)?;
        let model_config_id = args
            .default_model_config_id
            .or_else(|| self.default_model_config_id.clone())
            .ok_or_else(|| {
                "Select a local model configuration before creating a task".to_string()
            })?;
        if !self
            .models
            .iter()
            .any(|model| model["id"] == model_config_id)
        {
            return Err(format!(
                "Local model configuration is not available: {model_config_id}"
            ));
        }
        let enabled_builtin_kinds =
            self.validate_builtin_kinds(args.enabled_builtin_kinds, args.is_planning_task)?;
        let external_mcp_config_ids = validate_selected_ids(
            args.external_mcp_config_ids,
            if args.is_planning_task {
                &self.planning_external_mcp_ids
            } else {
                &self.execution_external_mcp_ids
            },
            "external MCP",
        )?;
        let selected_skill_ids = validate_selected_ids(
            args.selected_skill_ids,
            if args.is_planning_task {
                &self.planning_skill_ids
            } else {
                &self.execution_skill_ids
            },
            "Skill",
        )?;
        self.database
            .create_local_conversation_task(CreateLocalConversationTaskInput {
                owner_user_id: self.owner_user_id.clone(),
                project_id: self.project_id.clone(),
                session_id,
                source_turn_id,
                title: required_text(args.title, "title")?,
                description: args.description.unwrap_or_default().trim().to_string(),
                objective: required_text(args.objective, "objective")?,
                priority: args.priority.unwrap_or_default().clamp(-100, 100),
                tags: normalize_ids(args.tags),
                model_config_id,
                is_planning_task: args.is_planning_task,
                enabled_builtin_kinds,
                external_mcp_config_ids,
                selected_skill_ids,
                prerequisite_task_ids,
            })
            .await
            .map_err(|error| error.to_string())
    }

    fn validate_builtin_kinds(
        &self,
        values: Vec<String>,
        is_planning_task: bool,
    ) -> Result<Vec<String>, String> {
        let allowed_values = if is_planning_task {
            &self.planning_builtin_kinds
        } else {
            &self.selectable_builtin_kinds
        };
        let allowed = allowed_values
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut out = Vec::new();
        for value in normalize_ids(values) {
            let kind = builtin_kind_by_any(value.as_str())
                .ok_or_else(|| format!("Unknown local builtin capability: {value}"))?;
            let normalized = kind.kind_name().to_string();
            if !allowed.contains(normalized.as_str()) {
                return Err(format!(
                    "Plugin Management does not allow local Task Runner capability: {normalized}"
                ));
            }
            if !out.contains(&normalized) {
                out.push(normalized);
            }
        }
        let write = chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite
            .kind_name()
            .to_string();
        let read = chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerRead
            .kind_name()
            .to_string();
        if out.contains(&write) && !out.contains(&read) {
            return Err(
                "CodeMaintainerWrite requires CodeMaintainerRead to be selected explicitly"
                    .to_string(),
            );
        }
        Ok(out)
    }
}

#[async_trait]
impl BuiltinToolProvider for LocalTaskRunnerServiceProvider {
    fn server_name(&self) -> &str {
        SERVER_NAME
    }

    fn list_tools(&self) -> Vec<Value> {
        self.tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let payload = match name {
            "list_tasks" => {
                let args: ListTasksArgs = decode(args)?;
                let mut tasks = self
                    .database
                    .list_local_conversation_tasks(
                        self.owner_user_id.as_str(),
                        self.session_id.as_str(),
                        200,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                if let Some(status) = normalized(args.status) {
                    tasks.retain(|task| external_status(task.status.as_str()) == status);
                }
                if let Some(keyword) = normalized(args.keyword).map(|value| value.to_lowercase()) {
                    tasks.retain(|task| {
                        [
                            task.title.as_str(),
                            task.details.as_str(),
                            task.objective.as_str(),
                        ]
                        .iter()
                        .any(|value| value.to_lowercase().contains(keyword.as_str()))
                    });
                }
                let offset = args.offset.unwrap_or_default().min(tasks.len());
                let limit = args.limit.unwrap_or(100).clamp(1, 200);
                tasks.drain(0..offset);
                tasks.truncate(limit);
                json!(tasks.iter().map(task_value).collect::<Vec<_>>())
            }
            "get_task" => {
                let args: TaskIdArgs = decode(args)?;
                task_value(&require_task(self, args.task_id.as_str()).await?)
            }
            "create_task" => {
                let _ = self.source(&context)?;
                if let Some(existing) = self
                    .database
                    .first_local_conversation_task_for_turn(
                        self.owner_user_id.as_str(),
                        self.session_id.as_str(),
                        self.source_turn_id.as_str(),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                {
                    task_value(&existing)
                } else {
                    let decoded: CreateTaskArgs = decode(args)?;
                    let prerequisites = decoded.prerequisite_task_ids.clone();
                    task_value(&self.create_one(decoded, &context, prerequisites).await?)
                }
            }
            "list_mcp_builtin_catalog" => json!(self
                .selectable_builtin_kinds
                .iter()
                .chain(self.planning_builtin_kinds.iter())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|kind| json!({
                    "kind": kind,
                    "name": kind,
                    "runtime_provider": "local_connector",
                    "available_for_execution": self.selectable_builtin_kinds.contains(kind),
                    "available_for_planning": self.planning_builtin_kinds.contains(kind),
                }))
                .collect::<Vec<_>>()),
            "list_external_mcp_configs" => json!(self.external_mcp_configs),
            "list_available_skills" => json!(self.available_skills),
            "create_tasks_with_prerequisites" => {
                let _ = self.source(&context)?;
                let args: CreateTasksArgs = decode(args)?;
                if args.tasks.is_empty() {
                    return Err("tasks must not be empty".to_string());
                }
                let existing = self
                    .database
                    .list_local_conversation_tasks(
                        self.owner_user_id.as_str(),
                        self.session_id.as_str(),
                        200,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .filter(|task| task.source_turn_id == self.source_turn_id)
                    .collect::<Vec<_>>();
                if !existing.is_empty() {
                    json!(existing.iter().map(task_value).collect::<Vec<_>>())
                } else {
                    let mut pending = args.tasks;
                    let refs = pending
                        .iter()
                        .map(|item| item.client_ref.trim().to_string())
                        .collect::<Vec<_>>();
                    if refs.iter().any(|value| value.is_empty())
                        || refs.iter().collect::<BTreeSet<_>>().len() != refs.len()
                    {
                        return Err("client_ref values must be non-empty and unique".to_string());
                    }
                    let ref_set = refs.iter().map(String::as_str).collect::<BTreeSet<_>>();
                    for item in &pending {
                        for dependency in &item.prerequisite_refs {
                            if !ref_set.contains(dependency.trim()) {
                                return Err(format!("Unknown prerequisite_ref: {dependency}"));
                            }
                        }
                    }
                    let mut created_by_ref = BTreeMap::<String, LocalTaskBoardTaskRecord>::new();
                    while !pending.is_empty() {
                        let ready_index = pending.iter().position(|item| {
                            item.prerequisite_refs
                                .iter()
                                .all(|dependency| created_by_ref.contains_key(dependency.trim()))
                        });
                        let Some(index) = ready_index else {
                            return Err("Task prerequisite_refs contain a cycle".to_string());
                        };
                        let item = pending.remove(index);
                        let mut prerequisite_task_ids = item.task.prerequisite_task_ids.clone();
                        prerequisite_task_ids.extend(item.prerequisite_refs.iter().filter_map(
                            |value| created_by_ref.get(value.trim()).map(|task| task.id.clone()),
                        ));
                        prerequisite_task_ids = normalize_ids(prerequisite_task_ids);
                        let client_ref = item.client_ref.trim().to_string();
                        let created = self
                            .create_one(item.task, &context, prerequisite_task_ids)
                            .await?;
                        created_by_ref.insert(client_ref, created);
                    }
                    json!(refs
                        .iter()
                        .filter_map(|client_ref| created_by_ref.get(client_ref))
                        .map(task_value)
                        .collect::<Vec<_>>())
                }
            }
            "cancel_task" => {
                let args: CancelTaskArgs = decode(args)?;
                let task = require_task(self, args.task_id.as_str()).await?;
                if let Some(run_id) = task.last_run_id.as_deref() {
                    self.database
                        .request_local_task_run_cancel(self.owner_user_id.as_str(), run_id)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                let task = self
                    .database
                    .set_local_conversation_task_status(
                        self.owner_user_id.as_str(),
                        self.session_id.as_str(),
                        task.id.as_str(),
                        "cancelled",
                        None,
                        Some(args.reason.as_str()),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Local task was not found".to_string())?;
                task_value(&task)
            }
            "wait_for_task_completion" => json!({
                "accepted": true,
                "mode": "local_background",
                "message": "任务已交给客户端本地 Task Runner 后台执行。"
            }),
            "get_task_dependency_graph" => {
                let args: TaskIdArgs = decode(args)?;
                let task = require_task(self, args.task_id.as_str()).await?;
                let tasks = self
                    .database
                    .list_local_conversation_tasks(
                        self.owner_user_id.as_str(),
                        self.session_id.as_str(),
                        200,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let by_id = tasks
                    .iter()
                    .map(|item| (item.id.as_str(), item))
                    .collect::<BTreeMap<_, _>>();
                let dependencies = task
                    .prerequisite_task_ids
                    .iter()
                    .filter_map(|id| by_id.get(id.as_str()).copied())
                    .map(task_value)
                    .collect::<Vec<_>>();
                json!({"task": task_value(&task), "prerequisites": dependencies})
            }
            other => return Err(format!("Unknown local Task Runner tool: {other}")),
        };
        Ok(tool_result(payload))
    }
}

#[derive(Debug, Default, Deserialize)]
struct ListTasksArgs {
    status: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TaskIdArgs {
    task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CreateTaskArgs {
    title: String,
    #[serde(default)]
    description: Option<String>,
    objective: String,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    default_model_config_id: Option<String>,
    is_planning_task: bool,
    #[serde(default)]
    enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    selected_skill_ids: Vec<String>,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskWithRef {
    client_ref: String,
    #[serde(default)]
    prerequisite_refs: Vec<String>,
    #[serde(flatten)]
    task: CreateTaskArgs,
}

#[derive(Debug, Deserialize)]
struct CreateTasksArgs {
    #[serde(default)]
    tasks: Vec<CreateTaskWithRef>,
}

#[derive(Debug, Deserialize)]
struct CancelTaskArgs {
    task_id: String,
    reason: String,
}

async fn require_task(
    provider: &LocalTaskRunnerServiceProvider,
    task_id: &str,
) -> Result<LocalTaskBoardTaskRecord, String> {
    provider
        .database
        .get_local_task_board_task(
            provider.owner_user_id.as_str(),
            provider.session_id.as_str(),
            task_id,
        )
        .await
        .map_err(|error| error.to_string())?
        .filter(|task| task.task_kind == "task_runner")
        .ok_or_else(|| "Local Task Runner task was not found in this conversation".to_string())
}

fn local_external_mcp_views(
    capabilities: &chatos_plugin_management_sdk::ResolvedAgentCapabilities,
    manifests: &[LocalMcpManifestRecord],
) -> Vec<Value> {
    capabilities
        .selectable_mcps()
        .filter(|item| system_mcp_descriptor_for_record(&item.resource).is_none())
        .filter_map(|item| {
            let manifest = manifests.iter().find(|manifest| {
                manifest.plugin_mcp_id.as_deref() == Some(item.resource.id.as_str())
                    && manifest.is_locally_executable()
            })?;
            Some(json!({
                "id": item.resource.id,
                "name": item.resource.name,
                "display_name": item.resource.display_name,
                "description": item.resource.description,
                "manifest_id": manifest.manifest_id,
                "runtime_provider": "local_connector",
            }))
        })
        .collect()
}

fn validate_selected_ids(
    values: Vec<String>,
    allowed: &BTreeSet<String>,
    kind: &str,
) -> Result<Vec<String>, String> {
    let values = normalize_ids(values);
    for value in &values {
        if !allowed.contains(value) {
            return Err(format!(
                "Plugin Management does not allow local {kind}: {value}"
            ));
        }
    }
    Ok(values)
}

fn catalog_ids(catalog: &[Value]) -> BTreeSet<String> {
    catalog
        .iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn merge_catalogs(primary: Vec<Value>, secondary: Vec<Value>) -> Vec<Value> {
    let mut by_id = BTreeMap::<String, Value>::new();
    for item in primary.into_iter().chain(secondary) {
        if let Some(id) = item.get("id").and_then(Value::as_str) {
            by_id.entry(id.to_string()).or_insert(item);
        }
    }
    by_id.into_values().collect()
}

fn task_value(task: &LocalTaskBoardTaskRecord) -> Value {
    json!({
        "id": task.id,
        "title": task.title,
        "description": task.details,
        "objective": task.objective,
        "status": external_status(task.status.as_str()),
        "priority": priority_value(task.priority.as_str()),
        "tags": task.tags,
        "default_model_config_id": task.model_config_id,
        "is_planning_task": task.is_planning_task,
        "source_session_id": task.source_session_id,
        "source_turn_id": task.source_turn_id,
        "source_user_message_id": task.source_user_message_id,
        "prerequisite_task_ids": task.prerequisite_task_ids,
        "last_run_id": task.last_run_id,
        "result_summary": task.outcome_summary,
        "error": task.blocker_reason,
        "mcp_config": {
            "enabled_builtin_kinds": task.enabled_builtin_kinds,
            "external_mcp_config_ids": task.external_mcp_config_ids,
            "selected_skill_ids": task.selected_skill_ids,
        },
        "created_at": task.created_at,
        "updated_at": task.updated_at,
    })
}

fn external_status(status: &str) -> &str {
    match status {
        "todo" => "queued",
        "doing" => "running",
        "done" => "succeeded",
        "blocked" => "failed",
        "cancelled" | "canceled" => "cancelled",
        other => other,
    }
}

fn priority_value(priority: &str) -> i64 {
    match priority {
        "high" => 10,
        "low" => -10,
        _ => 0,
    }
}

fn normalize_ids(values: Vec<String>) -> Vec<String> {
    values.into_iter().fold(Vec::new(), |mut out, value| {
        let value = value.trim().to_string();
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
        out
    })
}

fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_text(value: String, field: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(value)
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": {"type": "object", "additionalProperties": true}
    })
}

fn empty_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn required_id_schema(field: &str) -> Value {
    json!({
        "type": "object",
        "properties": {field: {"type": "string", "minLength": 1}},
        "required": [field],
        "additionalProperties": false
    })
}

fn tool_result(payload: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        }],
        "structuredContent": payload,
        "isError": false
    })
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use chatos_agent::{AgentExecutor, AgentTurnMemory, AgentTurnRequest, COMMAND_APPROVAL_AGENT};
use chatos_ai_runtime::{
    MemoryContextComposer, MemoryEngineRecordWriter, MemoryRecordScope, MemoryScope,
    ModelRuntimeConfig,
};
use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, AgentPromptVendor, SystemAgentKey,
};

use crate::local_runtime::{
    database_path_for_state, load_installed_agent_prompt_from_database, LocalDatabase,
};
use crate::mcp::tools::code_maintainer_service_for_root;
use crate::workspace::paths::resolve_workspace_dir;
use crate::{local_now_rfc3339, LocalState};

use super::fingerprint::normalized_command;
use super::types::{ApprovalMemorySettings, CommandApprovalRequest};

const APPROVAL_DECISION_TOOL: &str = "approval_decision";

mod tool_executor;

use self::tool_executor::ApprovalAgentToolExecutor;

#[derive(Debug, Clone)]
pub(crate) enum AutoApprovalDecision {
    Approved { reason: String },
    Denied { reason: String },
    AskUser { reason: String },
}

#[derive(Clone)]
struct ApprovalAgentMemory {
    composer: MemoryContextComposer,
    writer: MemoryEngineRecordWriter,
    scope: MemoryScope,
    conversation_id: String,
}

pub(crate) async fn run_auto_approval_agent(
    state: &LocalState,
    state_path: &Path,
    request: &CommandApprovalRequest,
    risk_level: &str,
    risk_reason: Option<&str>,
) -> Result<AutoApprovalDecision> {
    let root = approval_project_root(state, request)?;
    let (model_config, prompt_vendor) = approval_model_config(
        state,
        request.project_key.owner_user_id.as_str(),
        root.as_path(),
    )?;
    let source_instance_id = state
        .auth
        .as_ref()
        .map(|auth| auth.cloud_base_url.trim_end_matches('/'))
        .ok_or_else(|| anyhow!("Local Connector login is required for Agent Prompt"))?;
    let database = LocalDatabase::open(database_path_for_state(state_path)).await?;
    let installed_prompt = load_installed_agent_prompt_from_database(
        &database,
        source_instance_id,
        SystemAgentKey::LocalConnectorCommandApprovalAgent,
        prompt_vendor,
    )
    .await?;
    let capability_policy = resolve_approval_capability_policy(state).await?;
    let code_service = code_maintainer_service_for_root(
        root.as_path(),
        Some(request.project_key.workspace_id.clone()),
        false,
        true,
        false,
    )?;
    let decision = Arc::new(Mutex::new(None));
    let executor = ApprovalAgentToolExecutor {
        code_service,
        decision: decision.clone(),
        allow_code_tools: capability_policy.code_maintainer_read,
        allow_approval_decision: capability_policy.approval_decision,
    };
    let memory = build_approval_agent_memory(
        &state.approval.memory,
        request,
        root.as_path(),
        state.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()),
        state.auth.as_ref().map(|auth| auth.access_token.as_str()),
    )
    .await?;
    let run_id = format!("approval-agent-{}", Uuid::new_v4());
    let conversation_id = memory
        .as_ref()
        .map(|memory| memory.conversation_id.clone())
        .unwrap_or_else(|| format!("local_connector_command_approval:{}", request.request_id));
    let mut prompt = build_approval_prompt(request, root.as_path(), risk_level, risk_reason)?;
    if let Some(provider_skills_prompt) = capability_policy
        .provider_skills_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\n");
        prompt.push_str(provider_skills_prompt);
    }
    let metadata = json!({
        "agent": "local_connector_command_approval_agent",
        "run_id": run_id,
        "request_id": request.request_id,
        "workspace_id": request.project_key.workspace_id,
        "project_id": request.project_key.project_id,
        "project_root_relative_path": request.project_key.project_root_relative_path,
        "project_anchor_relative_path": request.project_key.project_anchor_relative_path,
        "agent_prompt_bundle_version": installed_prompt.bundle_version,
        "agent_prompt_revision": installed_prompt.revision,
        "agent_prompt_checksum": installed_prompt.checksum,
    });
    let agent_memory = memory.as_ref().map(|memory| {
        AgentTurnMemory::new(
            memory.composer.clone(),
            memory.writer.clone(),
            memory.scope.clone(),
            memory.conversation_id.clone(),
        )
    });
    let retry_model_config = model_config.clone();
    let retry_conversation_id = conversation_id.clone();
    let retry_prompt_source = prompt.clone();
    let retry_executor = executor.clone();
    let retry_memory = agent_memory.clone();
    let retry_system_prompt = installed_prompt.content.clone();
    let retry_metadata_source = metadata.clone();
    let turn_request = AgentTurnRequest::new(model_config, conversation_id, run_id, prompt)
        .with_tool_executor(executor)
        .with_memory(agent_memory)
        .with_max_iterations(capability_policy.max_iterations)
        .with_system_prompt(installed_prompt.content)
        .with_metadata(metadata);
    AgentExecutor::new()
        .run(&COMMAND_APPROVAL_AGENT, turn_request)
        .await
        .map_err(|error| anyhow!(error.message().to_string()))?;

    if decision
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .is_none()
    {
        let retry_run_id = format!("approval-agent-retry-{}", Uuid::new_v4());
        let retry_prompt = format!(
            "{retry_prompt_source}\n\n上一轮没有调用 `{APPROVAL_DECISION_TOOL}`，因此没有形成有效审批结果。现在必须调用 `{APPROVAL_DECISION_TOOL}`，并且只能通过该工具返回 approve、deny 或 ask_user 之一；不要只输出文字结论。"
        );
        let mut retry_metadata = retry_metadata_source;
        retry_metadata["run_id"] = json!(retry_run_id);
        retry_metadata["retry_after_missing_decision"] = json!(true);
        let retry_request = AgentTurnRequest::new(
            retry_model_config,
            retry_conversation_id,
            retry_run_id,
            retry_prompt,
        )
        .with_tool_executor(retry_executor)
        .with_memory(retry_memory)
        .with_max_iterations(capability_policy.max_iterations)
        .with_system_prompt(retry_system_prompt)
        .with_metadata(retry_metadata);
        AgentExecutor::new()
            .run(&COMMAND_APPROVAL_AGENT, retry_request)
            .await
            .map_err(|error| anyhow!(error.message().to_string()))?;
    }

    let decision = decision
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .ok_or_else(|| anyhow!("AI did not call approval_decision"))?;
    Ok(match decision.decision.as_str() {
        "approve" => AutoApprovalDecision::Approved {
            reason: decision.reason,
        },
        "deny" => AutoApprovalDecision::Denied {
            reason: decision.reason,
        },
        "ask_user" => AutoApprovalDecision::AskUser {
            reason: decision.reason,
        },
        other => AutoApprovalDecision::AskUser {
            reason: format!("AI returned unsupported approval decision: {other}"),
        },
    })
}

#[derive(Debug, Deserialize)]
struct ApprovalCapabilityPolicy {
    policy_revision: String,
    #[serde(default = "default_agent_max_iterations")]
    max_iterations: usize,
    code_maintainer_read: bool,
    approval_decision: bool,
    #[serde(default)]
    provider_skills_prompt: Option<String>,
}

fn default_agent_max_iterations() -> usize {
    chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS
}

async fn resolve_approval_capability_policy(
    state: &LocalState,
) -> Result<ApprovalCapabilityPolicy> {
    let auth = state
        .auth
        .as_ref()
        .ok_or_else(|| anyhow!("Local Connector login is required for command approval policy"))?;
    let url = format!(
        "{}/api/plugin-management/agent-capabilities/local-command-approval",
        auth.cloud_base_url.trim().trim_end_matches('/')
    );
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build command approval policy client")?
        .get(url)
        .bearer_auth(auth.access_token.trim())
        .send()
        .await
        .context("request command approval capability policy")?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "command approval capability policy was rejected: {status}: {detail}"
        ));
    }
    let policy = response
        .json::<ApprovalCapabilityPolicy>()
        .await
        .context("decode command approval capability policy")?;
    if policy.policy_revision.trim().is_empty()
        || policy.max_iterations == 0
        || !policy.code_maintainer_read
        || !policy.approval_decision
    {
        return Err(anyhow!(
            "command approval required capabilities are unavailable"
        ));
    }
    Ok(policy)
}

fn approval_project_root(state: &LocalState, request: &CommandApprovalRequest) -> Result<PathBuf> {
    let workspace = state
        .workspace_by_id(request.project_key.workspace_id.as_str())
        .ok_or_else(|| {
            anyhow!(
                "workspace is not registered locally: {}",
                request.project_key.workspace_id
            )
        })?;
    resolve_workspace_dir(
        workspace,
        request.project_key.project_root_relative_path.as_str(),
    )
}

fn approval_model_config(
    state: &LocalState,
    owner_user_id: &str,
    root: &Path,
) -> Result<(ModelRuntimeConfig, AgentPromptVendor)> {
    let model_config_id = state
        .model_configs
        .settings
        .command_approval_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            state
                .model_configs
                .configs
                .iter()
                .find(|item| {
                    item.enabled
                        && !item.model.trim().is_empty()
                        && item
                            .api_key
                            .as_deref()
                            .map(str::trim)
                            .is_some_and(|value| !value.is_empty())
                })
                .map(|item| item.id.clone())
        })
        .ok_or_else(|| anyhow!("command approval model is not configured"))?;
    let runtime = crate::model_configs::resolve_local_model_runtime(
        state,
        owner_user_id,
        model_config_id.as_str(),
    )?;
    let thinking_level = state
        .model_configs
        .settings
        .command_approval_thinking_level
        .clone()
        .or(runtime.thinking_level);
    let provider = if runtime.provider.trim().is_empty() {
        "openai_compatible".to_string()
    } else {
        runtime.provider.trim().to_string()
    };
    let prompt_vendor =
        required_agent_prompt_vendor(runtime.prompt_vendor.as_deref(), provider.as_str())?;
    Ok((
        ModelRuntimeConfig::openai_compatible(
            runtime.base_url,
            runtime.api_key,
            runtime.model,
            provider,
        )
        .with_responses_support(runtime.supports_responses)
        .with_images_support(Some(runtime.supports_images))
        .with_temperature(runtime.temperature.or(Some(0.0)))
        .with_max_output_tokens(runtime.max_output_tokens.or(Some(1_200)))
        .with_thinking_level(thinking_level)
        .with_max_transient_retries(Some(runtime.model_request_max_retries))
        .with_request_cwd(Some(root.display().to_string())),
        prompt_vendor,
    ))
}

async fn build_approval_agent_memory(
    settings: &ApprovalMemorySettings,
    request: &CommandApprovalRequest,
    project_root: &Path,
    service_base_url: Option<&str>,
    user_access_token: Option<&str>,
) -> Result<Option<ApprovalAgentMemory>> {
    let service_base_url = service_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("approval memory requires Local Connector Service base url"))?;
    let user_access_token = user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("approval memory requires current user access token"))?;
    let base_url = local_connector_service_memory_engine_base_url(service_base_url);
    let source_id = "local_connector_approval";
    let timeout = Duration::from_millis(settings.timeout_ms.max(1_000));
    ensure_approval_memory_source(base_url.as_str(), timeout, source_id, user_access_token).await?;
    let client = memory_engine_sdk::MemoryEngineClient::new_direct(base_url, timeout, source_id)
        .map_err(anyhow::Error::msg)?
        .with_bearer_token(user_access_token.to_string());

    let thread_id = approval_memory_thread_id(request);
    let subject_id = approval_memory_subject_id(request);
    client
        .upsert_thread(
            thread_id.as_str(),
            &memory_engine_sdk::SdkUpsertThreadRequest {
                tenant_id: request.project_key.owner_user_id.clone(),
                subject_id: subject_id.clone(),
                thread_type: "local_connector_command_approval_agent".to_string(),
                external_thread_id: Some(request.project_key.workspace_id.clone()),
                title: Some(format!(
                    "Local command approval: {}",
                    request.project_key.project_root_relative_path
                )),
                labels: Some(vec![
                    "local_connector".to_string(),
                    "command_approval".to_string(),
                    format!("workspace:{}", request.project_key.workspace_id),
                ]),
                metadata: Some(json!({
                    "owner_service": "local_connector_client",
                    "agent": "local_connector_command_approval_agent",
                    "workspace_id": request.project_key.workspace_id,
                    "project_id": request.project_key.project_id,
                    "project_root_relative_path": request.project_key.project_root_relative_path,
                    "project_anchor_relative_path": request.project_key.project_anchor_relative_path,
                    "project_root": project_root.display().to_string(),
                    "updated_at": local_now_rfc3339(),
                })),
                status: Some("active".to_string()),
                created_at: None,
                updated_at: None,
                archived_at: None,
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;
    let composer = MemoryContextComposer::from_client(client.clone());
    let writer = MemoryEngineRecordWriter::from_client(
        client,
        MemoryRecordScope::message_thread(
            request.project_key.owner_user_id.clone(),
            thread_id.clone(),
        ),
    );
    Ok(Some(ApprovalAgentMemory {
        composer,
        writer,
        scope: MemoryScope::thread(
            request.project_key.owner_user_id.clone(),
            source_id.to_string(),
            thread_id.clone(),
        )
        .with_subject_id(subject_id),
        conversation_id: thread_id,
    }))
}

async fn ensure_approval_memory_source(
    base_url: &str,
    timeout: Duration,
    source_id: &str,
    user_access_token: &str,
) -> Result<()> {
    let client = memory_engine_sdk::MemoryEngineClient::new_platform(base_url.to_string(), timeout)
        .map_err(anyhow::Error::msg)?
        .with_bearer_token(user_access_token.to_string());
    client
        .upsert_source(
            source_id,
            &memory_engine_sdk::UpsertSourceRequest {
                tenant_id: None,
                source_type: "local_connector_approval_agent".to_string(),
                name: "Local Connector Command Approval Agent".to_string(),
                description: Some(
                    "Command approval agent managed by local_connector_client.".to_string(),
                ),
                config: Some(json!({
                    "platform_managed": true,
                    "owner_service": "local_connector_client",
                    "capabilities": [
                        "threads",
                        "records",
                        "context_compose",
                        "command_approval"
                    ],
                })),
                sdk_enabled: Some(true),
                status: Some("active".to_string()),
            },
        )
        .await
        .map(|_| ())
        .map_err(anyhow::Error::msg)
}

fn local_connector_service_memory_engine_base_url(service_base_url: &str) -> String {
    format!(
        "{}/api/local-connectors/memory-engine",
        service_base_url.trim_end_matches('/')
    )
}

fn approval_memory_thread_id(request: &CommandApprovalRequest) -> String {
    let payload = json!({
        "device_id": request.project_key.device_id,
        "workspace_id": request.project_key.workspace_id,
        "project_id": request.project_key.project_id,
        "project_root_relative_path": request.project_key.project_root_relative_path,
        "project_anchor_relative_path": request.project_key.project_anchor_relative_path,
    });
    format!(
        "local_connector_command_approval:{}",
        stable_short_hash(payload.to_string().as_str())
    )
}

fn approval_memory_subject_id(request: &CommandApprovalRequest) -> String {
    format!(
        "{}:{}",
        request.project_key.workspace_id,
        stable_short_hash(request.project_key.project_root_relative_path.as_str())
    )
}

fn stable_short_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize()).chars().take(24).collect()
}

fn build_approval_prompt(
    request: &CommandApprovalRequest,
    project_root: &Path,
    risk_level: &str,
    risk_reason: Option<&str>,
) -> Result<String> {
    let requested_permissions = request
        .requested_permissions
        .as_ref()
        .map(serde_json::to_string_pretty)
        .transpose()?
        .unwrap_or_else(|| "null".to_string());
    Ok(format!(
        r#"请审核下面这条本地 shell 命令是否可以执行。必要时先读取或搜索项目文件，再调用 `approval_decision` 给出最终结论。

审批请求：
- request_id: {request_id}
- source: {source}
- workspace_id: {workspace_id}
- project_id: {project_id}
- project_root_relative_path: {project_root_relative_path}
- project_anchor_relative_path: {project_anchor_relative_path}
- project_root: {project_root}
- cwd: {cwd}
- command: {command}
- requested_permissions: {requested_permissions}
- static_risk_level: {risk_level}
- static_risk_reason: {risk_reason}

审核重点：
- 命令是否符合当前项目的语言、包管理器、脚本和目录结构。
- 命令是否会访问 `.env`、私钥、token、系统目录或项目外路径。
- 临时权限是否是完成该命令所必需的最小范围；不要因为命令本身常见就忽略越界文件或网络权限。
- 命令是否包含破坏性删除、权限提升、远程脚本直接执行、生产基础设施操作等风险。
- 如果命令只是常见的只读检查、测试、构建、格式化、依赖安装等，也要结合项目文件确认合理性。
"#,
        request_id = request.request_id,
        source = request.source,
        workspace_id = request.project_key.workspace_id,
        project_id = request.project_key.project_id.as_deref().unwrap_or(""),
        project_root_relative_path = request.project_key.project_root_relative_path,
        project_anchor_relative_path = request
            .project_key
            .project_anchor_relative_path
            .as_deref()
            .unwrap_or(""),
        project_root = project_root.display(),
        cwd = request.cwd,
        command = normalized_command(request.command.as_str(), request.args.as_slice()),
        requested_permissions = requested_permissions,
        risk_level = risk_level,
        risk_reason = risk_reason.unwrap_or(""),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_thread_id_is_stable_for_same_project_key() {
        let request = CommandApprovalRequest {
            request_id: "req-1".to_string(),
            project_key: super::super::types::ApprovalProjectKey {
                owner_user_id: "user-1".to_string(),
                device_id: "device-1".to_string(),
                workspace_id: "workspace-1".to_string(),
                project_id: Some("project-1".to_string()),
                project_root_relative_path: ".".to_string(),
                project_anchor_relative_path: Some("Cargo.toml".to_string()),
            },
            command: "cargo".to_string(),
            args: vec!["test".to_string()],
            cwd: ".".to_string(),
            source: "test".to_string(),
            requested_permissions: None,
            session_id: Some("session-1".to_string()),
        };

        assert_eq!(
            approval_memory_thread_id(&request),
            approval_memory_thread_id(&request)
        );
    }

    #[test]
    fn model_config_requires_local_model_settings() {
        let err =
            approval_model_config(&LocalState::default(), "user-1", Path::new(".")).unwrap_err();

        assert!(err
            .to_string()
            .contains("command approval model is not configured"));
    }

    #[test]
    fn legacy_capability_response_uses_global_agent_default() {
        let policy = serde_json::from_value::<ApprovalCapabilityPolicy>(json!({
            "policy_revision": "revision-1",
            "code_maintainer_read": true,
            "approval_decision": true
        }))
        .expect("decode legacy policy");

        assert_eq!(
            policy.max_iterations,
            chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS
        );
    }
}

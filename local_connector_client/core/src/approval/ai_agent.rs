// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::json;
use uuid::Uuid;

use chatos_ai_runtime::{
    AiRuntime, ContextualTurnRunner, MemoryContextComposer, MemoryContextOverflowRecovery,
    MemoryEngineRecordWriter, MemoryRecordScope, MemoryScope, ModelRuntimeConfig,
    RuntimeRecordOptions, RuntimeTurnSpec, SaveRecordInput,
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
    Approved {
        reason: String,
        remember_allow: bool,
    },
    Denied {
        reason: String,
    },
    AskUser {
        reason: String,
    },
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
    request: &CommandApprovalRequest,
    risk_level: &str,
    risk_reason: Option<&str>,
) -> Result<AutoApprovalDecision> {
    let root = approval_project_root(state, request)?;
    let model_config = approval_model_config(
        state,
        request.project_key.owner_user_id.as_str(),
        root.as_path(),
    )?;
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
    };
    let memory = build_approval_agent_memory(
        &state.approval.memory,
        request,
        root.as_path(),
        state.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()),
        state.auth.as_ref().map(|auth| auth.access_token.as_str()),
    )
    .await?;
    let max_iterations = state
        .runtime_settings
        .clone()
        .normalized()
        .ai_agent_max_iterations;

    let mut runtime = AiRuntime::builder()
        .with_tool_executor(executor)
        .with_max_iterations(max_iterations);
    if let Some(memory) = memory.as_ref() {
        runtime = runtime.with_record_writer(memory.writer.clone());
    }
    let runner = ContextualTurnRunner::new(
        runtime.build_runtime(),
        memory.as_ref().map(|memory| memory.composer.clone()),
    )
    .with_context_overflow_recovery(Some(
        MemoryContextOverflowRecovery::new()
            .with_trigger_reason("local_connector_command_approval_context_overflow"),
    ));

    let run_id = format!("approval-agent-{}", Uuid::new_v4());
    let conversation_id = memory
        .as_ref()
        .map(|memory| memory.conversation_id.clone())
        .unwrap_or_else(|| format!("local_connector_command_approval:{}", request.request_id));
    let prompt = build_approval_prompt(request, root.as_path(), risk_level, risk_reason)?;
    let metadata = json!({
        "agent": "local_connector_command_approval_agent",
        "run_id": run_id,
        "request_id": request.request_id,
        "workspace_id": request.project_key.workspace_id,
        "project_id": request.project_key.project_id,
        "project_root_relative_path": request.project_key.project_root_relative_path,
        "project_anchor_relative_path": request.project_key.project_anchor_relative_path,
    });
    let user_record = memory.as_ref().map(|memory| {
        SaveRecordInput::user_message(memory.conversation_id.clone(), prompt.clone())
            .with_conversation_turn_id(run_id.clone())
            .with_message_mode("local_connector_command_approval_agent")
            .with_message_source("local_connector_client")
            .with_metadata(metadata.clone())
    });
    let record_options = RuntimeRecordOptions::persist_all()
        .with_assistant_message_mode("local_connector_command_approval_agent")
        .with_assistant_message_source("local_connector_client")
        .with_assistant_metadata(metadata.clone())
        .with_tool_message_mode("local_connector_command_approval_agent")
        .with_tool_message_source("local_connector_client")
        .with_tool_metadata(metadata);
    let mut agent_model_config = model_config;
    agent_model_config.instructions = Some(approval_agent_system_prompt(
        agent_model_config.instructions.as_deref(),
    ));
    let spec = RuntimeTurnSpec::for_user_text(agent_model_config.clone(), conversation_id, prompt)
        .with_conversation_turn_id(run_id)
        .with_caller_model(agent_model_config.model.clone())
        .with_record_options(record_options)
        .with_memory_scope(memory.as_ref().map(|memory| memory.scope.clone()))
        .with_user_record(user_record);
    runner
        .run_turn(spec.into_contextual_turn_request())
        .await
        .map_err(anyhow::Error::msg)?;

    let decision = decision
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .ok_or_else(|| anyhow!("AI did not call approval_decision"))?;
    Ok(match decision.decision.as_str() {
        "approve" => AutoApprovalDecision::Approved {
            reason: decision.reason,
            remember_allow: decision.remember_allow,
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
) -> Result<ModelRuntimeConfig> {
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
    Ok(ModelRuntimeConfig::openai_compatible(
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
    .with_request_cwd(Some(root.display().to_string())))
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

fn approval_agent_system_prompt(existing: Option<&str>) -> String {
    let fixed = r#"你是 Local Connector Client 内置的命令审批 Agent。你的唯一职责是在本地项目范围内审核即将执行的 shell 命令是否可以放行。

你可以使用文件读取、目录列表和文本搜索工具了解项目上下文。你不能执行命令，不能修改文件，不能联网，不能请求额外工具。

最后必须调用 `approval_decision` 工具返回结论：
- `approve`：命令与当前项目上下文匹配，风险可接受，且不会读取/泄露敏感信息、破坏数据、越权修改系统或项目外文件。
- `deny`：命令明显危险，包括破坏性删除/覆盖、权限提升、读取或外传密钥、远程脚本管道执行、修改系统目录、不可逆基础设施操作等。
- `ask_user`：缺少业务意图、影响范围不清、需要用户确认，或你无法通过本地文件判断。

如果选择 `approve`，只有当命令非常稳定且低风险时才把 `remember_allow` 设为 true。"#;
    existing
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}\n\n{fixed}"))
        .unwrap_or_else(|| fixed.to_string())
}

fn build_approval_prompt(
    request: &CommandApprovalRequest,
    project_root: &Path,
    risk_level: &str,
    risk_reason: Option<&str>,
) -> Result<String> {
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
- static_risk_level: {risk_level}
- static_risk_reason: {risk_reason}

审核重点：
- 命令是否符合当前项目的语言、包管理器、脚本和目录结构。
- 命令是否会访问 `.env`、私钥、token、系统目录或项目外路径。
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
}

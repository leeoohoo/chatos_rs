// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde_json::json;
use uuid::Uuid;

use chatos_agent::{AgentExecutor, AgentTurnRequest, COMMAND_APPROVAL_AGENT};
use chatos_ai_runtime::{ModelRuntimeConfig, ToolExecutor};
use chatos_plugin_management_sdk::{
    required_agent_prompt_vendor, AgentPromptVendor, SystemAgentKey, SystemMcpKey,
    LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID, SYSTEM_MCP_RUNTIME_KIND,
};

use crate::local_runtime::{
    database_path_for_state, load_installed_agent_prompt_from_database, LocalDatabase,
};
use crate::mcp::tools::code_maintainer_service_for_root;
use crate::workspace::paths::resolve_workspace_dir;
use crate::LocalState;

use super::decision_tool::APPROVAL_DECISION_TOOL;
use super::fingerprint::normalized_command;
use super::types::CommandApprovalRequest;

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

pub(crate) async fn run_auto_approval_agent(
    state: &LocalState,
    state_path: &Path,
    request: &CommandApprovalRequest,
    risk_level: &str,
    risk_reason: Option<&str>,
) -> Result<AutoApprovalDecision> {
    let root = approval_project_root(state, request)?;
    let (model_config, prompt_vendor) =
        approval_model_config(state, request.project_key.owner_user_id.as_str())?;
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
    let capability_policy = resolve_local_approval_capability_policy(
        &database,
        request.project_key.owner_user_id.as_str(),
    )
    .await?;
    let decision = Arc::new(Mutex::new(None));
    let mut prompt = build_approval_prompt(request, root.as_path(), risk_level, risk_reason)?;
    let run_id = format!("approval-agent-{}", Uuid::new_v4());
    let conversation_id = format!("local_connector_command_approval:{}", request.request_id);
    let code_service = code_maintainer_service_for_root(
        root.as_path(),
        Some(request.project_key.workspace_id.clone()),
        Some(conversation_id.clone()),
        Some(run_id.clone()),
        false,
        true,
        false,
    )?;
    let tool_executor: Arc<dyn ToolExecutor> = Arc::new(ApprovalAgentToolExecutor {
        code_service,
        decision: decision.clone(),
        allow_code_tools: capability_policy.code_maintainer_read,
        allow_approval_decision: capability_policy.approval_decision,
    });
    let max_iterations = capability_policy.max_iterations;
    let provider_skills_prompt = capability_policy.provider_skills_prompt.clone();
    if let Some(provider_skills_prompt) = provider_skills_prompt
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
        "tool_plane": "local_only",
        "capability_policy_revision": capability_policy.policy_revision,
        "agent_prompt_bundle_version": installed_prompt.bundle_version,
        "agent_prompt_revision": installed_prompt.revision,
        "agent_prompt_checksum": installed_prompt.checksum,
    });
    let retry_model_config = model_config.clone();
    let retry_conversation_id = conversation_id.clone();
    let retry_prompt_source = prompt.clone();
    let retry_executor = tool_executor.clone();
    let retry_system_prompt = installed_prompt.content.clone();
    let retry_metadata_source = metadata.clone();
    let turn_request = AgentTurnRequest::new(model_config, conversation_id, run_id, prompt)
        .with_tool_executor_arc(tool_executor)
        .with_max_iterations(max_iterations)
        .with_system_prompt(installed_prompt.content)
        .with_metadata(metadata);
    let mut execution_result = AgentExecutor::new()
        .run(&COMMAND_APPROVAL_AGENT, turn_request)
        .await
        .map(|_| ())
        .map_err(|error| anyhow!(error.message().to_string()));

    // approval_decision is the terminal, authoritative output of this agent.
    // The generic agent runtime may still ask for a displayable assistant
    // message after the tool call and report an empty-final-response error.
    // Do not discard a decision that the tool executor already validated and
    // persisted merely because that presentation-only follow-up was empty.
    if let Some(decision) = captured_approval_decision(&decision) {
        return Ok(auto_approval_decision(decision));
    }

    if execution_result.is_ok() && captured_approval_decision(&decision).is_none() {
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
        .with_tool_executor_arc(retry_executor)
        .with_max_iterations(max_iterations)
        .with_system_prompt(retry_system_prompt)
        .with_metadata(retry_metadata);
        execution_result = AgentExecutor::new()
            .run(&COMMAND_APPROVAL_AGENT, retry_request)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!(error.message().to_string()));
    }

    if let Some(decision) = captured_approval_decision(&decision) {
        return Ok(auto_approval_decision(decision));
    }
    execution_result?;
    Err(anyhow!("AI did not call approval_decision"))
}

fn captured_approval_decision(
    decision: &Arc<Mutex<Option<super::decision_tool::ApprovalToolDecision>>>,
) -> Option<super::decision_tool::ApprovalToolDecision> {
    decision.lock().ok().and_then(|guard| guard.clone())
}

fn auto_approval_decision(
    decision: super::decision_tool::ApprovalToolDecision,
) -> AutoApprovalDecision {
    match decision.decision.as_str() {
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
    }
}

struct LocalApprovalCapabilityPolicy {
    policy_revision: String,
    max_iterations: usize,
    code_maintainer_read: bool,
    approval_decision: bool,
    provider_skills_prompt: Option<String>,
}

async fn resolve_local_approval_capability_policy(
    database: &LocalDatabase,
    owner_user_id: &str,
) -> Result<LocalApprovalCapabilityPolicy> {
    let capabilities = database
        .get_capability_snapshot(
            owner_user_id,
            SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str(),
        )
        .await?
        .ok_or_else(|| anyhow!("local command approval capability snapshot is not installed"))?;
    if !capabilities.agent_enabled || capabilities.policy_revision.trim().is_empty() {
        return Err(anyhow!(
            "command approval required local capabilities are unavailable"
        ));
    }
    capabilities
        .ensure_required_runtime_supported([], [])
        .map_err(|error| anyhow!(error.to_string()))?;
    let code_maintainer_read = capabilities.mcps.iter().any(|item| {
        item.binding.required
            && item.available
            && item.resource.runtime.kind == SYSTEM_MCP_RUNTIME_KIND
            && item.resource.runtime.system_key.as_deref()
                == Some(SystemMcpKey::CodeMaintainerRead.as_str())
    });
    let approval_decision = capabilities
        .require_available_mcp(LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID)
        .is_ok();
    if !code_maintainer_read || !approval_decision {
        return Err(anyhow!(
            "command approval required local capabilities are unavailable"
        ));
    }
    let provider_skills_prompt = capabilities.compose_provider_skills_prompt(
        [
            SystemMcpKey::CodeMaintainerRead.as_str(),
            LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
        ],
        Some("zh-CN"),
    );
    Ok(LocalApprovalCapabilityPolicy {
        policy_revision: capabilities.policy_revision,
        max_iterations: chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS,
        code_maintainer_read,
        approval_decision,
        provider_skills_prompt,
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
        // Command approval is on the critical path of a tool call. Do not let
        // provider retry storms consume the entire relay lifetime; the caller
        // has a hard deadline and fails closed if this short retry is exhausted.
        .with_max_transient_retries(Some(runtime.model_request_max_retries.min(1))),
        prompt_vendor,
    ))
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
    let cwd = approval_cwd_for_prompt(request.cwd.as_str(), project_root);
    Ok(format!(
        r#"请审核下面这条本地 shell 命令是否可以执行。必要时先读取或搜索项目文件，再调用 `approval_decision` 给出最终结论。

审批请求：
- source: {source}
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

复用建议：
- 对确定性、低风险、无临时权限请求的项目内命令，如果后续重复执行不需要重新审查，请在 `approve` 时设置 `remember_allow: true`。
- 适合记住的例子包括版本/工具链查询、只读包元数据查询、项目脚本中已有的测试/构建/格式化命令，以及固定 cwd 下的相同安全命令。
- 不要为读取密钥或 `.env`、系统目录操作、破坏性命令、远程脚本执行、生产/集群操作、项目外路径、或带 requested_permissions 的命令设置 `remember_allow`。
"#,
        source = request.source,
        cwd = cwd,
        command = normalized_command(request.command.as_str(), request.args.as_slice()),
        requested_permissions = requested_permissions,
        risk_level = risk_level,
        risk_reason = risk_reason.unwrap_or(""),
    ))
}

fn approval_cwd_for_prompt(cwd: &str, project_root: &Path) -> String {
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return ".".to_string();
    }
    let cwd_path = Path::new(cwd);
    if !cwd_path.is_absolute() {
        return cwd.to_string();
    }
    cwd_path
        .strip_prefix(project_root)
        .ok()
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.to_string_lossy().into_owned()
            }
        })
        .unwrap_or_else(|| "<项目外路径>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn approval_prompt_does_not_expose_routing_identity_or_absolute_project_root() {
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
            redact_arguments_in_history: false,
            cwd: "/private/work/project/crates/core".to_string(),
            source: "test".to_string(),
            requested_permissions: None,
            session_id: Some("session-1".to_string()),
            action_audit: None,
        };

        let prompt =
            build_approval_prompt(&request, Path::new("/private/work/project"), "low", None)
                .expect("approval prompt");

        assert!(prompt.contains("cwd: crates/core"));
        assert!(!prompt.contains("/private/work/project"));
        assert!(!prompt.contains("workspace-1"));
        assert!(!prompt.contains("project-1"));
        assert!(!prompt.contains("device-1"));
    }

    #[test]
    fn approval_tool_remember_allow_reaches_auto_decision() {
        let (decision, _) = super::super::decision_tool::approval_decision_tool_result(json!({
            "decision": "approve",
            "reason": "stable project test command",
            "remember_allow": true
        }))
        .expect("approval decision");

        assert!(matches!(
            auto_approval_decision(decision),
            AutoApprovalDecision::Approved {
                remember_allow: true,
                ..
            }
        ));
    }

    #[test]
    fn model_config_requires_local_model_settings() {
        let err = approval_model_config(&LocalState::default(), "user-1").unwrap_err();

        assert!(err
            .to_string()
            .contains("command approval model is not configured"));
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemAgentKey;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity, SystemAgentDefinition};

pub const COMMAND_APPROVAL_AGENT: CommandApprovalAgent = CommandApprovalAgent;

#[derive(Debug, Default, Clone, Copy)]
pub struct CommandApprovalAgent;

impl AgentIdentity for CommandApprovalAgent {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(SystemAgentKey::LocalConnectorCommandApprovalAgent)
    }
}

impl SystemAgentDefinition for CommandApprovalAgent {
    fn system_prompt(&self) -> &'static str {
        r#"你是 Local Connector Client 内置的命令审批 Agent。你的唯一职责是在本地项目范围内审核即将执行的 shell 命令是否可以放行。

你可以使用文件读取、目录列表和文本搜索工具了解项目上下文。你不能执行命令，不能修改文件，不能联网，不能请求额外工具。

最后必须调用 `approval_decision` 工具返回结论：
- `approve`：命令与当前项目上下文匹配，风险可接受，且不会读取/泄露敏感信息、破坏数据、越权修改系统或项目外文件。
- `deny`：命令明显危险，包括破坏性删除/覆盖、权限提升、读取或外传密钥、远程脚本管道执行、修改系统目录、不可逆基础设施操作等。
- `ask_user`：缺少业务意图、影响范围不清、需要用户确认，或你无法通过本地文件判断。

如果选择 `approve`，只有当命令非常稳定且低风险时才把 `remember_allow` 设为 true。"#
    }

    fn message_mode(&self) -> &'static str {
        "local_connector_command_approval_agent"
    }

    fn message_source(&self) -> &'static str {
        "local_connector_client"
    }

    fn max_iterations(&self) -> usize {
        8
    }

    fn context_overflow_trigger(&self) -> &'static str {
        "local_connector_command_approval_context_overflow"
    }

    fn default_temperature(&self) -> Option<f64> {
        Some(0.0)
    }

    fn default_max_output_tokens(&self) -> Option<i64> {
        Some(1_200)
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemAgentKey;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity, SystemAgentDefinition};

pub const PROJECT_ENVIRONMENT_AGENT: ProjectEnvironmentAgent = ProjectEnvironmentAgent;

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectEnvironmentAgent;

impl AgentIdentity for ProjectEnvironmentAgent {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(SystemAgentKey::ProjectManagementAgent)
    }
}

impl SystemAgentDefinition for ProjectEnvironmentAgent {
    fn system_prompt(&self) -> &'static str {
        "你是 Project Management Service 内置的运行环境初始化 Agent。你的业务范围固定：读取当前项目文件，判断项目是否可运行，识别运行时和依赖服务，使用沙箱镜像 MCP 搜索或同步创建所需镜像，然后通过项目环境工具写入当前项目的运行环境结果。不要处理需求拆解、任务执行、代码修改或其它项目管理任务。"
    }

    fn message_mode(&self) -> &'static str {
        "project_environment_agent"
    }

    fn message_source(&self) -> &'static str {
        "project_management_service"
    }

    fn max_iterations(&self) -> usize {
        600
    }

    fn context_overflow_trigger(&self) -> &'static str {
        "project_environment_agent_context_overflow"
    }

    fn default_temperature(&self) -> Option<f64> {
        Some(0.1)
    }

    fn default_max_output_tokens(&self) -> Option<i64> {
        Some(4_000)
    }
}

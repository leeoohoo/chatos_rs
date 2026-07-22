// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::{BuiltinMcpKind, BuiltinMcpPromptLocale};
use chatos_plugin_management_sdk::SystemMcpKey;
use serde::{Deserialize, Serialize};

use crate::system_mcp_descriptor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemMcpProviderSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub locale: Option<String>,
}

pub fn system_mcp_provider_skills(key: SystemMcpKey) -> Vec<SystemMcpProviderSkill> {
    let descriptor = system_mcp_descriptor(key);
    if let Some(kind) = descriptor.embedded_kind {
        return builtin_provider_skills(kind, descriptor.display_name);
    }
    service_provider_skill(key).into_iter().collect()
}

fn builtin_provider_skills(
    kind: BuiltinMcpKind,
    display_name: &str,
) -> Vec<SystemMcpProviderSkill> {
    [
        (BuiltinMcpPromptLocale::ZhCn, "zh-CN", "zh_cn", "使用指南"),
        (
            BuiltinMcpPromptLocale::EnUs,
            "en-US",
            "en_us",
            "Usage Guide",
        ),
    ]
    .into_iter()
    .filter_map(|(locale, locale_key, suffix, name_suffix)| {
        let instructions =
            chatos_mcp_runtime::builtin_mcp_provider_skill_instructions(kind, locale)?;
        let description = if locale.is_english() {
            format!("Guidance for using the {display_name} tools exposed in the current run.")
        } else {
            format!("指导 AI 使用本轮实际暴露的 {display_name} 工具。")
        };
        Some(SystemMcpProviderSkill {
            id: format!("{}_usage_{suffix}", kind.server_name()),
            name: format!("{display_name} {name_suffix}"),
            description,
            instructions,
            locale: Some(locale_key.to_string()),
        })
    })
    .collect()
}

fn service_provider_skill(key: SystemMcpKey) -> Option<SystemMcpProviderSkill> {
    let (id, name, description, instructions) = match key {
        SystemMcpKey::SandboxImages => (
            "sandbox_images_usage",
            "Sandbox Images MCP 使用指南",
            "指导 AI 搜索、复用和创建项目沙箱镜像，并只采用工具真实返回的镜像结果。",
            include_str!("../provider_skills/sandbox-images.md"),
        ),
        SystemMcpKey::ProjectEnvironment => (
            "project_environment_usage",
            "Project Environment MCP 使用指南",
            "指导 AI 读取和更新当前项目的运行环境状态。",
            include_str!("../provider_skills/project-environment.md"),
        ),
        SystemMcpKey::ProjectRuntimeEnvironment => (
            "project_runtime_environment_usage",
            "项目运行环境信息 MCP 使用指南",
            "指导 Task Runner 执行 Agent 读取当前项目已经初始化好的环境信息。",
            include_str!("../provider_skills/project-runtime-environment.md"),
        ),
        SystemMcpKey::LocalCommandApproval => (
            "local_command_approval_usage",
            "Local Command Approval MCP 使用指南",
            "指导 AI 根据当前项目证据完成本地命令审批，不执行命令或修改文件。",
            include_str!("../provider_skills/local-command-approval.md"),
        ),
        SystemMcpKey::TaskRunnerService => (
            "task_runner_usage",
            "Task Runner MCP 使用指南",
            "指导 AI 把当前用户和项目需求交给内部异步执行链路，并正确选择 MCP 与 Local Connector Skills。",
            include_str!("../provider_skills/task-runner-service.md"),
        ),
        _ => return None,
    };
    Some(SystemMcpProviderSkill {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        instructions: instructions.trim().to_string(),
        locale: None,
    })
}

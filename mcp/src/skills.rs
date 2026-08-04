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
    if key == SystemMcpKey::TaskManager {
        return Vec::new();
    }
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
            "运行镜像工具使用指南",
            "指导 AI 搜索和复用项目运行镜像，并只采用工具真实返回的镜像结果。",
            include_str!("../provider_skills/sandbox-images.md"),
        ),
        SystemMcpKey::ProjectEnvironment => (
            "project_environment_usage",
            "项目环境分析工具使用指南",
            "指导 AI 读取和更新当前项目的运行环境状态。",
            include_str!("../provider_skills/project-environment.md"),
        ),
        SystemMcpKey::ProjectRuntimeEnvironment => (
            "project_runtime_environment_usage",
            "项目运行环境信息工具使用指南",
            "指导执行 Agent 读取当前项目已经初始化好的环境信息。",
            include_str!("../provider_skills/project-runtime-environment.md"),
        ),
        SystemMcpKey::LocalCommandApproval => (
            "local_command_approval_usage",
            "本地命令审批工具使用指南",
            "指导 AI 根据当前项目证据完成本地命令审批，不执行命令或修改文件。",
            include_str!("../provider_skills/local-command-approval.md"),
        ),
        SystemMcpKey::TaskProcessLog => (
            "task_process_log_usage",
            "任务过程记录工具使用指南",
            "指导执行 Agent 记录简短、可展示的当前任务执行过程。",
            include_str!("../provider_skills/task-process-log.md"),
        ),
        SystemMcpKey::TaskRunnerService => (
            "task_runner_usage",
            "异步任务工具使用指南",
            "指导 AI 把当前用户和项目需求安排为可持续执行和回传结果的后台任务。",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_facing_service_guidance_hides_execution_routing_internals() {
        for key in [
            SystemMcpKey::TaskRunnerService,
            SystemMcpKey::ProjectRuntimeEnvironment,
            SystemMcpKey::TaskProcessLog,
        ] {
            let guidance = service_provider_skill(key).expect("service tool guidance");
            let content = format!(
                "{}\n{}\n{}",
                guidance.name, guidance.description, guidance.instructions
            );
            for forbidden in [
                "MCP",
                "Local Connector",
                "Harness",
                "Provider",
                "execution plane",
                "Runtime Session",
            ] {
                assert!(!content.contains(forbidden), "{key:?}: {forbidden}");
            }
        }
    }
}

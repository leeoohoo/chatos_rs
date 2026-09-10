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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_profiles: Vec<String>,
}

pub fn system_mcp_provider_skills(key: SystemMcpKey) -> Vec<SystemMcpProviderSkill> {
    if key == SystemMcpKey::TaskManager {
        return Vec::new();
    }
    if key == SystemMcpKey::TaskRunnerService {
        return vec![task_runner_provider_skill()];
    }
    let descriptor = system_mcp_descriptor(key);
    if let Some(kind) = descriptor.embedded_kind {
        return builtin_provider_skills(kind, descriptor.display_name);
    }
    service_provider_skill(key).into_iter().collect()
}

pub fn task_runner_provider_skill() -> SystemMcpProviderSkill {
    SystemMcpProviderSkill {
        id: "task_runner_usage".to_string(),
        name: "异步任务工具使用指南".to_string(),
        description: "指导 AI 把当前用户和项目需求安排为可持续执行和回传结果的后台任务。"
            .to_string(),
        instructions: include_str!("../provider_skills/task-runner-service.md")
            .trim()
            .to_string(),
        locale: None,
        task_profiles: vec!["default".to_string()],
    }
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
            task_profiles: Vec::new(),
        })
    })
    .collect()
}

fn service_provider_skill(key: SystemMcpKey) -> Option<SystemMcpProviderSkill> {
    let (id, name, description, instructions) = match key {
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
        _ => return None,
    };
    Some(SystemMcpProviderSkill {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        instructions: instructions.trim().to_string(),
        locale: None,
        task_profiles: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_facing_service_guidance_hides_execution_routing_internals() {
        for key in [
            SystemMcpKey::TaskRunnerService,
            SystemMcpKey::TaskProcessLog,
        ] {
            let guidance = system_mcp_provider_skills(key);
            assert!(!guidance.is_empty(), "service tool guidance");
            for guidance in guidance {
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

    #[test]
    fn task_runner_guidance_has_no_planning_mode_variant() {
        let skills = system_mcp_provider_skills(SystemMcpKey::TaskRunnerService);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].task_profiles, ["default"]);
        assert!(!skills[0].instructions.contains("规划模式"));
        assert!(skills[0].instructions.contains("wait_for_task_completion"));
    }
}

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

/// Stable product Skill coverage for one model-visible system MCP tool.
///
/// The catalog lives beside the system MCP provider guidance so every runtime consumes the same
/// tool-to-Skill mapping instead of inferring coverage from exposed tool-name prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemMcpProductSkillBinding {
    pub binding_id: &'static str,
    pub primary_skill: &'static str,
    pub required_skills: &'static [&'static str],
    pub coverage_revision: u32,
}

const PROJECT_READ_SKILLS: &[&str] = &["chatos-project-files", "chatos-project-read"];
const PROJECT_WRITE_SKILLS: &[&str] = &["chatos-project-files", "chatos-project-write"];
const TERMINAL_COMMAND_SKILLS: &[&str] = &["chatos-terminal", "chatos-terminal-command-execution"];
const TERMINAL_OBSERVATION_SKILLS: &[&str] =
    &["chatos-terminal", "chatos-terminal-process-observation"];
const TERMINAL_CONTROL_SKILLS: &[&str] = &["chatos-terminal", "chatos-terminal-process-control"];
const REQUIREMENT_SURVEY_ROUTER_SKILLS: &[&str] = &["requirement-survey"];
const REQUIREMENT_SURVEY_CREATE_SKILLS: &[&str] =
    &["requirement-survey", "requirement-survey-create"];
const REQUIREMENT_SURVEY_READ_SKILLS: &[&str] =
    &["requirement-survey", "requirement-survey-read-results"];
const REQUIREMENT_SURVEY_RESOLVE_SKILLS: &[&str] =
    &["requirement-survey", "requirement-survey-resolve"];
const REQUIREMENT_SURVEY_REVIEW_SKILLS: &[&str] =
    &["requirement-survey", "requirement-survey-review-execution"];
const AGENT_BUILDER_SKILLS: &[&str] = &["chatos-agent-builder"];
const REMOTE_CONNECTION_SKILLS: &[&str] = &["chatos-remote-connection"];
const USER_CLARIFICATION_SKILLS: &[&str] = &["chatos-user-clarification"];
const NOTEPAD_SKILLS: &[&str] = &["chatos-notepad"];
const MEMORY_CONTEXT_SKILLS: &[&str] = &["chatos-memory-context"];
const COMMAND_APPROVAL_SKILLS: &[&str] = &["chatos-command-approval"];
const TASK_PROGRESS_SKILLS: &[&str] = &["chatos-task-progress"];
const ASYNC_TASK_ORCHESTRATION_SKILLS: &[&str] = &["chatos-async-task-orchestration"];

const fn product_binding(
    binding_id: &'static str,
    primary_skill: &'static str,
    required_skills: &'static [&'static str],
) -> SystemMcpProductSkillBinding {
    SystemMcpProductSkillBinding {
        binding_id,
        primary_skill,
        required_skills,
        coverage_revision: 1,
    }
}

/// Resolve the immutable product Skill binding for a concrete system MCP tool.
///
/// Tool names are deliberately enumerated. Adding a new tool to a covered catalog must update
/// this mapping and its coverage test instead of silently inheriting a broad family wildcard.
pub fn system_mcp_product_skill_binding(
    key: SystemMcpKey,
    tool_name: &str,
) -> Option<SystemMcpProductSkillBinding> {
    let tool_name = tool_name.trim();
    match (key, tool_name) {
        (
            SystemMcpKey::CodeMaintainerRead | SystemMcpKey::CodeMaintainerWrite,
            "read_file_raw" | "read_file_range" | "list_dir" | "search_text" | "read_file"
            | "search_files",
        ) => Some(product_binding(
            "project-files.read",
            "chatos-project-read",
            PROJECT_READ_SKILLS,
        )),
        (
            SystemMcpKey::CodeMaintainerWrite,
            "open_edit_session" | "stage_edit_batch" | "commit_edit_session" | "abort_edit_session",
        ) => Some(product_binding(
            "project-files.write",
            "chatos-project-write",
            PROJECT_WRITE_SKILLS,
        )),
        (SystemMcpKey::TerminalController, "execute_command") => Some(product_binding(
            "terminal.command-execution",
            "chatos-terminal-command-execution",
            TERMINAL_COMMAND_SKILLS,
        )),
        (
            SystemMcpKey::TerminalController,
            "get_recent_logs" | "process_list" | "process_poll" | "process_log" | "process_wait",
        ) => Some(product_binding(
            "terminal.process-observation",
            "chatos-terminal-process-observation",
            TERMINAL_OBSERVATION_SKILLS,
        )),
        (SystemMcpKey::TerminalController, "process_write" | "process_kill" | "process") => {
            Some(product_binding(
                "terminal.process-control",
                "chatos-terminal-process-control",
                TERMINAL_CONTROL_SKILLS,
            ))
        }
        (
            SystemMcpKey::RequirementSurveyRead,
            "skill_activate" | "skill_list_resources" | "skill_read_resource",
        ) => Some(product_binding(
            "requirement-survey.control-plane",
            "requirement-survey",
            REQUIREMENT_SURVEY_ROUTER_SKILLS,
        )),
        (
            SystemMcpKey::RequirementSurveyRead,
            "requirement_survey_list" | "requirement_survey_get",
        ) => Some(product_binding(
            "requirement-survey.read-results",
            "requirement-survey-read-results",
            REQUIREMENT_SURVEY_READ_SKILLS,
        )),
        (SystemMcpKey::RequirementSurveyRead, "requirement_survey_project_tasks") => {
            Some(product_binding(
                "requirement-survey.review-execution",
                "requirement-survey-review-execution",
                REQUIREMENT_SURVEY_REVIEW_SKILLS,
            ))
        }
        (SystemMcpKey::RequirementSurveyWrite, "requirement_survey_create") => {
            Some(product_binding(
                "requirement-survey.create",
                "requirement-survey-create",
                REQUIREMENT_SURVEY_CREATE_SKILLS,
            ))
        }
        (SystemMcpKey::RequirementSurveyWrite, "requirement_survey_resolve") => {
            Some(product_binding(
                "requirement-survey.resolve",
                "requirement-survey-resolve",
                REQUIREMENT_SURVEY_RESOLVE_SKILLS,
            ))
        }
        (
            SystemMcpKey::AgentBuilder,
            "recommend_agent_profile"
            | "create_memory_agent"
            | "update_memory_agent"
            | "preview_agent_context",
        ) => Some(product_binding(
            "agent-builder",
            "chatos-agent-builder",
            AGENT_BUILDER_SKILLS,
        )),
        (
            SystemMcpKey::RemoteConnectionController,
            "test_connection" | "run_command" | "list_directory" | "read_file" | "download_file"
            | "upload_file",
        ) => Some(product_binding(
            "remote-connection",
            "chatos-remote-connection",
            REMOTE_CONNECTION_SKILLS,
        )),
        (SystemMcpKey::AskUser, "prompt_key_values" | "prompt_choices" | "prompt_mixed_form") => {
            Some(product_binding(
                "user-clarification.forms",
                "chatos-user-clarification",
                USER_CLARIFICATION_SKILLS,
            ))
        }
        (
            SystemMcpKey::Notepad,
            "init" | "list_folders" | "create_folder" | "rename_folder" | "delete_folder"
            | "list_notes" | "create_note" | "read_note" | "update_note" | "delete_note"
            | "list_tags" | "search_notes",
        ) => Some(product_binding(
            "notepad.durable-notes",
            "chatos-notepad",
            NOTEPAD_SKILLS,
        )),
        (SystemMcpKey::MemorySkillReader, "get_skill_detail")
        | (SystemMcpKey::MemoryCommandReader, "get_command_detail")
        | (SystemMcpKey::MemoryPluginReader, "get_plugin_detail") => Some(product_binding(
            "memory-context.expansion",
            "chatos-memory-context",
            MEMORY_CONTEXT_SKILLS,
        )),
        (SystemMcpKey::LocalCommandApproval, "approval_decision") => Some(product_binding(
            "command-approval.decision",
            "chatos-command-approval",
            COMMAND_APPROVAL_SKILLS,
        )),
        (SystemMcpKey::TaskProcessLog, "record_process" | "report_outcome") => {
            Some(product_binding(
                "task-progress.reporting",
                "chatos-task-progress",
                TASK_PROGRESS_SKILLS,
            ))
        }
        (
            SystemMcpKey::TaskRunnerService,
            "list_tasks"
            | "get_task"
            | "create_task"
            | "create_tasks_with_prerequisites"
            | "cancel_task"
            | "wait_for_task_completion"
            | "get_task_dependency_graph",
        ) => Some(product_binding(
            "task-runner.orchestration",
            "chatos-async-task-orchestration",
            ASYNC_TASK_ORCHESTRATION_SKILLS,
        )),
        _ => None,
    }
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
    fn centrally_skilled_system_tool_catalogs_have_explicit_bindings() {
        for key in [
            SystemMcpKey::CodeMaintainerRead,
            SystemMcpKey::CodeMaintainerWrite,
            SystemMcpKey::TerminalController,
            SystemMcpKey::RequirementSurveyRead,
            SystemMcpKey::RequirementSurveyWrite,
            SystemMcpKey::AgentBuilder,
            SystemMcpKey::RemoteConnectionController,
            SystemMcpKey::AskUser,
            SystemMcpKey::Notepad,
            SystemMcpKey::MemorySkillReader,
            SystemMcpKey::MemoryCommandReader,
            SystemMcpKey::MemoryPluginReader,
            SystemMcpKey::LocalCommandApproval,
            SystemMcpKey::TaskProcessLog,
        ] {
            for tool in crate::system_mcp_static_tools(key).expect("static tool catalog") {
                let tool_name = tool
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .expect("tool name");
                if key == SystemMcpKey::RemoteConnectionController
                    && tool_name == "list_connections"
                {
                    assert!(system_mcp_product_skill_binding(key, tool_name).is_none());
                    continue;
                }
                assert!(
                    system_mcp_product_skill_binding(key, tool_name).is_some(),
                    "{} tool {tool_name} has no central product Skill binding",
                    key.as_str()
                );
            }
        }
    }

    #[test]
    fn unknown_tools_do_not_inherit_a_family_binding() {
        assert!(system_mcp_product_skill_binding(
            SystemMcpKey::TerminalController,
            "future_unreviewed_tool"
        )
        .is_none());
    }

    #[test]
    fn task_runner_model_tools_have_explicit_bindings() {
        for tool_name in [
            "list_tasks",
            "get_task",
            "create_task",
            "create_tasks_with_prerequisites",
            "cancel_task",
            "wait_for_task_completion",
            "get_task_dependency_graph",
        ] {
            assert!(
                system_mcp_product_skill_binding(SystemMcpKey::TaskRunnerService, tool_name)
                    .is_some()
            );
        }
        assert!(system_mcp_product_skill_binding(
            SystemMcpKey::TaskRunnerService,
            "admin_only_future_tool"
        )
        .is_none());
    }

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
        assert!(skills[0]
            .instructions
            .contains("不是等待任务终态的轮询函数"));
        assert!(skills[0].instructions.contains("本轮不得再调用"));
        assert!(skills[0].instructions.contains("以联系人第一人称自然说明"));
        assert!(skills[0].instructions.contains("不得向用户提及 Task"));
    }

    #[test]
    fn task_process_guidance_requires_continuous_visible_updates() {
        let skills = system_mcp_provider_skills(SystemMcpKey::TaskProcessLog);
        assert_eq!(skills.len(), 1);
        assert!(skills[0].instructions.contains("过程记录应贯穿任务"));
        assert!(skills[0].instructions.contains("关键步骤和阶段变化"));
        assert!(skills[0].instructions.contains("不要为每次工具调用"));
    }
}

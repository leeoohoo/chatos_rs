use crate::services::memory_server_client::MemoryAgentRuntimeContextDto;

use super::types::{
    contact_plugin_ref, contact_skill_ref, ContactSkillPromptMode, ParsedContactCommandInvocation,
    CONTACT_COMMAND_READER_TOOL_NAME, CONTACT_PLUGIN_READER_TOOL_NAME,
    CONTACT_SKILL_READER_TOOL_NAME,
};

pub fn compose_contact_command_system_prompt(
    command: Option<&ParsedContactCommandInvocation>,
) -> Option<String> {
    let command = command?;
    if command.command_ref.trim().is_empty()
        || command.plugin_source.trim().is_empty()
        || command.source_path.trim().is_empty()
    {
        return None;
    }

    let mut lines = vec![
        "用户在本轮显式触发了联系人命令，请优先按照命令内容执行。".to_string(),
        format!("command_ref={}", command.command_ref.trim()),
        format!("命令名称={}", command.name.trim()),
        format!("plugin_source={}", command.plugin_source.trim()),
        format!("source_path={}", command.source_path.trim()),
    ];
    if let Some(description) = command.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            lines.push(format!("命令简介={}", description));
        }
    }
    if let Some(argument_hint) = command.argument_hint.as_deref().map(str::trim) {
        if !argument_hint.is_empty() {
            lines.push(format!("参数提示={}", argument_hint));
        }
    }
    if let Some(arguments) = command.arguments.as_deref().map(str::trim) {
        if !arguments.is_empty() {
            lines.push(format!("用户附加参数={}", arguments));
        }
    }
    let content = command.content.trim();
    if !content.is_empty() {
        lines.push("命令完整内容：".to_string());
        for item in content.lines() {
            lines.push(item.to_string());
        }
    }
    Some(lines.join("\n").trim().to_string())
}

pub fn compose_contact_system_prompt(
    runtime_context: Option<&MemoryAgentRuntimeContextDto>,
    skill_mode: &ContactSkillPromptMode,
) -> Option<String> {
    #[derive(Clone)]
    struct SkillPromptEntry {
        skill_ref: String,
        name: Option<String>,
        plugin_source: Option<String>,
        description: Option<String>,
        source_type: String,
    }
    #[derive(Clone)]
    struct PluginPromptEntry {
        plugin_ref: String,
        name: Option<String>,
    }

    let agent = runtime_context?;
    let agent_name = agent.name.trim();
    if agent_name.is_empty() {
        return None;
    }

    let mut lines = vec![
        "你正在以联系人智能体身份参与对话。".to_string(),
        format!("联系人名称：{}", agent_name),
    ];

    if let Some(description) = agent.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            lines.push(format!("联系人简介：{}", description));
        }
    }
    if let Some(category) = agent.category.as_deref().map(str::trim) {
        if !category.is_empty() {
            lines.push(format!("联系人分类：{}", category));
        }
    }

    lines.push(String::new());
    lines.push("角色定义：".to_string());
    lines.push(agent.role_definition.trim().to_string());

    let mut skill_entries: Vec<SkillPromptEntry>;
    let mut plugin_entries: Vec<PluginPromptEntry>;

    match skill_mode {
        ContactSkillPromptMode::Disabled => {
            skill_entries = Vec::new();
            plugin_entries = Vec::new();
            lines.push(String::new());
            lines.push("技能上下文：本轮用户未开启技能，不要假设可用技能/插件内容，也不要为了技能详情调用技能或插件查询工具。".to_string());
        }
        ContactSkillPromptMode::Summary { force_skill_first } => {
            skill_entries = Vec::new();
            plugin_entries = Vec::new();
            if *force_skill_first {
                lines.push(String::new());
                lines.push("技能优先规则：本轮用户开启了技能但未指定某个技能；你必须先阅读下面的关联技能/插件/命令概览，判断是否有适用技能。只要有适用技能，优先按技能说明执行；只有概览不足以完成任务时，才调用技能/插件/命令 reader 工具获取全文。".to_string());
            }

            lines.push(String::new());
            lines.push("关联技能（使用 skill_ref，避免长随机ID）：".to_string());
            if !agent.runtime_skills.is_empty() {
                for (index, skill) in agent.runtime_skills.iter().enumerate() {
                    let entry = SkillPromptEntry {
                        skill_ref: contact_skill_ref(index),
                        name: normalize_optional_string(Some(skill.name.clone())),
                        plugin_source: skill
                            .plugin_source
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToOwned::to_owned),
                        description: skill
                            .description
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToOwned::to_owned),
                        source_type: skill.source_type.trim().to_string(),
                    };
                    let mut parts = vec![format!("skill_ref={}", entry.skill_ref)];
                    if let Some(name) = entry.name.as_deref() {
                        parts.push(format!("名称={}", name));
                    }
                    if let Some(plugin_source) = entry.plugin_source.as_deref() {
                        parts.push(format!("plugin_source={}", plugin_source));
                    }
                    parts.push(format!(
                        "简介={}",
                        entry.description.as_deref().unwrap_or("未提供")
                    ));
                    parts.push(format!("来源类型={}", entry.source_type));
                    lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
                    skill_entries.push(entry);
                }
            } else if !agent.skill_ids.is_empty() {
                for (index, _skill_id) in agent.skill_ids.iter().enumerate() {
                    let entry = SkillPromptEntry {
                        skill_ref: contact_skill_ref(index),
                        name: None,
                        plugin_source: None,
                        description: None,
                        source_type: "skill_center".to_string(),
                    };
                    lines.push(format!(
                        "{}. skill_ref={} | 简介=未提供 | 来源类型={} | 详情可通过工具查询",
                        index + 1,
                        entry.skill_ref,
                        entry.source_type
                    ));
                    skill_entries.push(entry);
                }
            } else {
                lines.push("无".to_string());
            }

            lines.push(String::new());
            lines.push("关联插件（使用 plugin_ref，仅给简介）：".to_string());
            if !agent.runtime_plugins.is_empty() {
                for (index, plugin) in agent.runtime_plugins.iter().enumerate() {
                    let plugin_source = plugin.source.trim().to_string();
                    let mut parts = vec![
                        format!("plugin_ref={}", contact_plugin_ref(index)),
                        format!("plugin_source={}", plugin_source),
                        format!("名称={}", plugin.name.trim()),
                    ];
                    if let Some(category) = plugin.category.as_deref().map(str::trim) {
                        if !category.is_empty() {
                            parts.push(format!("分类={}", category));
                        }
                    }
                    let description = plugin
                        .description
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .or_else(|| {
                            plugin
                                .content_summary
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                        })
                        .unwrap_or("未提供");
                    parts.push(format!("简介={}", description));
                    let related_skills = skill_entries
                        .iter()
                        .filter(|entry| {
                            entry
                                .plugin_source
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(|value| value == plugin_source)
                                .unwrap_or(false)
                        })
                        .map(|entry| {
                            let skill_name = entry.name.as_deref().unwrap_or("未命名技能");
                            format!("{}({})", entry.skill_ref, skill_name)
                        })
                        .collect::<Vec<_>>();
                    if !related_skills.is_empty() {
                        parts.push(format!("覆盖技能={}", related_skills.join(", ")));
                    }
                    lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
                    plugin_entries.push(PluginPromptEntry {
                        plugin_ref: contact_plugin_ref(index),
                        name: normalize_optional_string(Some(plugin.name.clone())),
                    });
                }
            } else if !agent.plugin_sources.is_empty() {
                for (index, source) in agent.plugin_sources.iter().enumerate() {
                    let source = source.trim().to_string();
                    let related_skills = skill_entries
                        .iter()
                        .filter(|entry| {
                            entry
                                .plugin_source
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(|value| value == source)
                                .unwrap_or(false)
                        })
                        .map(|entry| {
                            let skill_name = entry.name.as_deref().unwrap_or("未命名技能");
                            format!("{}({})", entry.skill_ref, skill_name)
                        })
                        .collect::<Vec<_>>();
                    let mut parts = vec![
                        format!("plugin_ref={}", contact_plugin_ref(index)),
                        format!("plugin_source={}", source),
                        "简介=未提供".to_string(),
                    ];
                    if !related_skills.is_empty() {
                        parts.push(format!("覆盖技能={}", related_skills.join(", ")));
                    }
                    lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
                    plugin_entries.push(PluginPromptEntry {
                        plugin_ref: contact_plugin_ref(index),
                        name: None,
                    });
                }
            } else {
                lines.push("无".to_string());
            }
        }
        ContactSkillPromptMode::SelectedFull { skills, plugins } => {
            skill_entries = Vec::with_capacity(skills.len());
            plugin_entries = Vec::with_capacity(plugins.len());
            lines.push(String::new());
            lines.push("技能优先规则：本轮用户已显式选择技能。下面已经包含所选技能及其所属插件的全文；你必须优先按照这些全文执行，不要再调用技能/插件查询工具，也不要使用未列出的技能。".to_string());

            lines.push(String::new());
            lines.push("已选择技能全文：".to_string());
            if skills.is_empty() {
                lines.push("无（用户开启了技能选择，但没有有效选择项）".to_string());
            } else {
                for (index, skill) in skills.iter().enumerate() {
                    let mut parts = vec![
                        format!("skill_ref={}", skill.skill_ref),
                        format!("skill_id={}", skill.id),
                        format!("名称={}", skill.name),
                        format!("来源类型={}", skill.source_type),
                    ];
                    if let Some(plugin_source) = skill.plugin_source.as_deref() {
                        parts.push(format!("plugin_source={}", plugin_source));
                    }
                    if let Some(source_path) = skill.source_path.as_deref() {
                        parts.push(format!("source_path={}", source_path));
                    }
                    if let Some(description) = skill.description.as_deref() {
                        parts.push(format!("简介={}", description));
                    }
                    if let Some(updated_at) = skill.updated_at.as_deref() {
                        parts.push(format!("updated_at={}", updated_at));
                    }
                    lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
                    lines.push("内容：".to_string());
                    let content = skill.content.trim();
                    if content.is_empty() {
                        lines.push("（空）".to_string());
                    } else {
                        for item in content.lines() {
                            lines.push(item.to_string());
                        }
                    }
                    skill_entries.push(SkillPromptEntry {
                        skill_ref: skill.skill_ref.clone(),
                        name: Some(skill.name.clone()),
                        plugin_source: skill.plugin_source.clone(),
                        description: skill.description.clone(),
                        source_type: skill.source_type.clone(),
                    });
                    lines.push(String::new());
                }
            }

            lines.push("所选技能关联插件全文：".to_string());
            if plugins.is_empty() {
                lines.push("无".to_string());
            } else {
                for (index, plugin) in plugins.iter().enumerate() {
                    let mut parts = vec![
                        format!("plugin_ref={}", plugin.plugin_ref),
                        format!("plugin_source={}", plugin.source),
                        format!("名称={}", plugin.name),
                    ];
                    if let Some(category) = plugin.category.as_deref() {
                        parts.push(format!("分类={}", category));
                    }
                    if let Some(description) = plugin.description.as_deref() {
                        parts.push(format!("简介={}", description));
                    }
                    if let Some(version) = plugin.version.as_deref() {
                        parts.push(format!("version={}", version));
                    }
                    if let Some(repository) = plugin.repository.as_deref() {
                        parts.push(format!("repository={}", repository));
                    }
                    if let Some(branch) = plugin.branch.as_deref() {
                        parts.push(format!("branch={}", branch));
                    }
                    if let Some(updated_at) = plugin.updated_at.as_deref() {
                        parts.push(format!("updated_at={}", updated_at));
                    }
                    lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
                    let content = plugin.content.as_deref().map(str::trim).unwrap_or("");
                    if !content.is_empty() {
                        lines.push("插件内容：".to_string());
                        for item in content.lines() {
                            lines.push(item.to_string());
                        }
                    }
                    if !plugin.commands.is_empty() {
                        lines.push("插件命令：".to_string());
                        for command in &plugin.commands {
                            let mut command_parts = vec![
                                format!("名称={}", command.name.trim()),
                                format!("source_path={}", command.source_path.trim()),
                            ];
                            if let Some(description) = command.description.as_deref().map(str::trim)
                            {
                                if !description.is_empty() {
                                    command_parts.push(format!("简介={}", description));
                                }
                            }
                            if let Some(argument_hint) =
                                command.argument_hint.as_deref().map(str::trim)
                            {
                                if !argument_hint.is_empty() {
                                    command_parts.push(format!("参数提示={}", argument_hint));
                                }
                            }
                            lines.push(format!("- {}", command_parts.join(" | ")));
                            let command_content = command.content.trim();
                            if !command_content.is_empty() {
                                lines.push("  内容：".to_string());
                                for item in command_content.lines() {
                                    lines.push(format!("  {}", item));
                                }
                            }
                        }
                    }
                    plugin_entries.push(PluginPromptEntry {
                        plugin_ref: plugin.plugin_ref.clone(),
                        name: Some(plugin.name.clone()),
                    });
                    lines.push(String::new());
                }
            }
        }
    }

    if matches!(skill_mode, ContactSkillPromptMode::Summary { .. }) {
        lines.push(String::new());
        lines.push("关联命令（使用 command_ref）：".to_string());
        if !agent.runtime_commands.is_empty() {
            for (index, command) in agent.runtime_commands.iter().enumerate() {
                let mut parts = vec![
                    format!("command_ref={}", command.command_ref.trim()),
                    format!("名称={}", command.name.trim()),
                    format!("plugin_source={}", command.plugin_source.trim()),
                ];
                parts.push(format!(
                    "简介={}",
                    command
                        .description
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("未提供")
                ));
                if let Some(argument_hint) = command.argument_hint.as_deref().map(str::trim) {
                    if !argument_hint.is_empty() {
                        parts.push(format!("参数提示={}", argument_hint));
                    }
                }
                if let Some(source_path) =
                    normalize_optional_string(Some(command.source_path.clone()))
                {
                    parts.push(format!("source_path={}", source_path));
                }
                lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
            }
        } else {
            lines.push("无".to_string());
        }

        if !agent.runtime_commands.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "如果需要查看某个 command 的完整内容，请调用内置工具 `{}`，仅传 `command_ref`（如 `CMD1`）。",
                CONTACT_COMMAND_READER_TOOL_NAME
            ));
        }
    }

    if matches!(skill_mode, ContactSkillPromptMode::Summary { .. }) && !plugin_entries.is_empty() {
        lines.push(String::new());
        let plugin_examples = plugin_entries
            .iter()
            .take(3)
            .map(|entry| match entry.name.as_deref() {
                Some(name) => format!("{}({})", entry.plugin_ref, name),
                None => entry.plugin_ref.clone(),
            })
            .collect::<Vec<_>>();
        if plugin_examples.is_empty() {
            lines.push(format!(
                "如果需要查看某个 plugin 的完整内容，请调用内置工具 `{}`，仅传 `plugin_ref`（如 `PL1`）。",
                CONTACT_PLUGIN_READER_TOOL_NAME
            ));
        } else {
            lines.push(format!(
                "如果需要查看某个 plugin 的完整内容，请调用内置工具 `{}`，仅传 `plugin_ref`（如 {}）。",
                CONTACT_PLUGIN_READER_TOOL_NAME,
                plugin_examples.join(", ")
            ));
        }
    }

    if matches!(skill_mode, ContactSkillPromptMode::Summary { .. }) && !skill_entries.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "如果需要查看某个 skill 的完整内容，请调用内置工具 `{}`，仅传 `skill_ref`（如 `SK1`）。",
            CONTACT_SKILL_READER_TOOL_NAME
        ));
    }

    Some(lines.join("\n").trim().to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

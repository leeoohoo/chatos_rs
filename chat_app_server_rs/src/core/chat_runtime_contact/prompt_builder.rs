use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::chatos_agent_types::ChatosAgentRuntimeContextDto;

use super::types::ContactSkillPromptMode;
#[cfg(test)]
use super::types::ParsedContactCommandInvocation;
#[cfg(test)]
use super::types::{
    contact_plugin_ref, contact_skill_ref, CONTACT_COMMAND_READER_TOOL_NAME,
    CONTACT_PLUGIN_READER_TOOL_NAME, CONTACT_SKILL_READER_TOOL_NAME,
};

#[cfg(test)]
pub fn compose_contact_command_system_prompt(
    command: Option<&ParsedContactCommandInvocation>,
    locale: InternalContextLocale,
) -> Option<String> {
    let command = command?;
    if command.command_ref.trim().is_empty()
        || command.plugin_source.trim().is_empty()
        || command.source_path.trim().is_empty()
    {
        return None;
    }

    let mut lines = vec![
        text(
            locale,
            "用户在本轮显式触发了联系人命令，请优先按照命令内容执行。",
            "The user explicitly triggered a contact command in this turn. Follow the command content with priority.",
        ),
        format!("command_ref={}", command.command_ref.trim()),
        format!(
            "{}={}",
            field(locale, "命令名称", "command_name"),
            command.name.trim()
        ),
        format!("plugin_source={}", command.plugin_source.trim()),
        format!("source_path={}", command.source_path.trim()),
    ];
    if let Some(description) = command.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            lines.push(format!(
                "{}={}",
                field(locale, "命令简介", "command_description"),
                description
            ));
        }
    }
    if let Some(argument_hint) = command.argument_hint.as_deref().map(str::trim) {
        if !argument_hint.is_empty() {
            lines.push(format!(
                "{}={}",
                field(locale, "参数提示", "argument_hint"),
                argument_hint
            ));
        }
    }
    if let Some(arguments) = command.arguments.as_deref().map(str::trim) {
        if !arguments.is_empty() {
            lines.push(format!(
                "{}={}",
                field(locale, "用户附加参数", "user_arguments"),
                arguments
            ));
        }
    }
    let content = command.content.trim();
    if !content.is_empty() {
        lines.push(text(locale, "命令完整内容：", "Full command content:"));
        for item in content.lines() {
            lines.push(item.to_string());
        }
    }
    Some(lines.join("\n").trim().to_string())
}

pub fn compose_contact_system_prompt(
    runtime_context: Option<&ChatosAgentRuntimeContextDto>,
    skill_mode: &ContactSkillPromptMode,
    locale: InternalContextLocale,
) -> Option<String> {
    #[cfg(test)]
    #[derive(Clone)]
    struct SkillPromptEntry {
        skill_ref: String,
        name: Option<String>,
        plugin_source: Option<String>,
        description: Option<String>,
        source_type: String,
    }
    #[cfg(test)]
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
        text(
            locale,
            "你正在以联系人智能体身份参与对话。",
            "You are participating in this conversation as a contact agent.",
        ),
        format!(
            "{}{}",
            text(locale, "联系人名称：", "Contact name: "),
            agent_name
        ),
    ];

    if let Some(description) = agent.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            lines.push(format!(
                "{}{}",
                text(locale, "联系人简介：", "Contact description: "),
                description
            ));
        }
    }
    if let Some(category) = agent.category.as_deref().map(str::trim) {
        if !category.is_empty() {
            lines.push(format!(
                "{}{}",
                text(locale, "联系人分类：", "Contact category: "),
                category
            ));
        }
    }

    lines.push(String::new());
    lines.push(text(locale, "角色定义：", "Role definition:"));
    lines.push(agent.role_definition.trim().to_string());

    #[cfg(test)]
    let mut skill_entries: Vec<SkillPromptEntry> = Vec::new();
    #[cfg(test)]
    let mut plugin_entries: Vec<PluginPromptEntry> = Vec::new();

    match skill_mode {
        ContactSkillPromptMode::Disabled => {
            lines.push(String::new());
            lines.push(text(
                locale,
                "技能上下文：本轮只使用任务系统提供的能力，不加载联系人技能或插件内容。",
                "Skill context: this turn only uses Task Runner capabilities; contact skills and plugin content are not loaded.",
            ));
        }
        #[cfg(test)]
        ContactSkillPromptMode::Summary { force_skill_first } => {
            skill_entries = Vec::new();
            plugin_entries = Vec::new();
            if *force_skill_first {
                lines.push(String::new());
                lines.push(text(
                    locale,
                    "技能优先规则：本轮用户开启了技能但未指定某个技能；你必须先阅读下面的关联技能/插件/命令概览，判断是否有适用技能。只要有适用技能，优先按技能说明执行；只有概览不足以完成任务时，才调用技能/插件/命令 reader 工具获取全文。",
                    "Skill-first rule: the user enabled skills in this turn but did not select a specific skill. You must first review the related skill/plugin/command summaries below and decide whether any skill applies. If a relevant skill exists, follow that skill first. Only call skill/plugin/command reader tools when the summary is not enough to complete the task.",
                ));
            }

            lines.push(String::new());
            lines.push(text(
                locale,
                "关联技能（使用 skill_ref，避免长随机ID）：",
                "Related skills (use skill_ref instead of long random IDs):",
            ));
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
                        parts.push(format!("{}={}", field(locale, "名称", "name"), name));
                    }
                    if let Some(plugin_source) = entry.plugin_source.as_deref() {
                        parts.push(format!("plugin_source={}", plugin_source));
                    }
                    parts.push(format!(
                        "{}={}",
                        field(locale, "简介", "description"),
                        entry.description.as_deref().unwrap_or(text_ref(
                            locale,
                            "未提供",
                            "not provided"
                        ))
                    ));
                    parts.push(format!(
                        "{}={}",
                        field(locale, "来源类型", "source_type"),
                        entry.source_type
                    ));
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
                        "{}. skill_ref={} | {}={} | {}={} | {}",
                        index + 1,
                        entry.skill_ref,
                        field(locale, "简介", "description"),
                        text(locale, "未提供", "not provided"),
                        field(locale, "来源类型", "source_type"),
                        entry.source_type,
                        text(
                            locale,
                            "详情可通过工具查询",
                            "details can be queried with tools"
                        )
                    ));
                    skill_entries.push(entry);
                }
            } else {
                lines.push(text(locale, "无", "None"));
            }

            lines.push(String::new());
            lines.push(text(
                locale,
                "关联插件（使用 plugin_ref，仅给简介）：",
                "Related plugins (use plugin_ref, summary only):",
            ));
            if !agent.runtime_plugins.is_empty() {
                for (index, plugin) in agent.runtime_plugins.iter().enumerate() {
                    let plugin_source = plugin.source.trim().to_string();
                    let mut parts = vec![
                        format!("plugin_ref={}", contact_plugin_ref(index)),
                        format!("plugin_source={}", plugin_source),
                        format!("{}={}", field(locale, "名称", "name"), plugin.name.trim()),
                    ];
                    if let Some(category) = plugin.category.as_deref().map(str::trim) {
                        if !category.is_empty() {
                            parts.push(format!(
                                "{}={}",
                                field(locale, "分类", "category"),
                                category
                            ));
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
                        .unwrap_or(text_ref(locale, "未提供", "not provided"));
                    parts.push(format!(
                        "{}={}",
                        field(locale, "简介", "description"),
                        description
                    ));
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
                            let skill_name = entry.name.as_deref().unwrap_or(text_ref(
                                locale,
                                "未命名技能",
                                "unnamed skill",
                            ));
                            format!("{}({})", entry.skill_ref, skill_name)
                        })
                        .collect::<Vec<_>>();
                    if !related_skills.is_empty() {
                        parts.push(format!(
                            "{}={}",
                            field(locale, "覆盖技能", "covered_skills"),
                            related_skills.join(", ")
                        ));
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
                            let skill_name = entry.name.as_deref().unwrap_or(text_ref(
                                locale,
                                "未命名技能",
                                "unnamed skill",
                            ));
                            format!("{}({})", entry.skill_ref, skill_name)
                        })
                        .collect::<Vec<_>>();
                    let mut parts = vec![
                        format!("plugin_ref={}", contact_plugin_ref(index)),
                        format!("plugin_source={}", source),
                        format!(
                            "{}={}",
                            field(locale, "简介", "description"),
                            text(locale, "未提供", "not provided")
                        ),
                    ];
                    if !related_skills.is_empty() {
                        parts.push(format!(
                            "{}={}",
                            field(locale, "覆盖技能", "covered_skills"),
                            related_skills.join(", ")
                        ));
                    }
                    lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
                    plugin_entries.push(PluginPromptEntry {
                        plugin_ref: contact_plugin_ref(index),
                        name: None,
                    });
                }
            } else {
                lines.push(text(locale, "无", "None"));
            }
        }
        #[cfg(test)]
        ContactSkillPromptMode::SelectedFull { skills, plugins } => {
            skill_entries = Vec::with_capacity(skills.len());
            plugin_entries = Vec::with_capacity(plugins.len());
            lines.push(String::new());
            lines.push(text(
                locale,
                "技能优先规则：本轮用户已显式选择技能。下面已经包含所选技能及其所属插件的全文；你必须优先按照这些全文执行，不要再调用技能/插件查询工具，也不要使用未列出的技能。",
                "Skill-first rule: the user explicitly selected skills in this turn. The full text of the selected skills and their related plugins is already included below. You must prioritize these full texts, do not call skill/plugin reader tools again, and do not use skills that are not listed here.",
            ));

            lines.push(String::new());
            lines.push(text(
                locale,
                "已选择技能全文：",
                "Full text of selected skills:",
            ));
            if skills.is_empty() {
                lines.push(text(
                    locale,
                    "无（用户开启了技能选择，但没有有效选择项）",
                    "None (the user enabled skill selection, but there were no valid selected items)",
                ));
            } else {
                for (index, skill) in skills.iter().enumerate() {
                    let mut parts = vec![
                        format!("skill_ref={}", skill.skill_ref),
                        format!("skill_id={}", skill.id),
                        format!("{}={}", field(locale, "名称", "name"), skill.name),
                        format!(
                            "{}={}",
                            field(locale, "来源类型", "source_type"),
                            skill.source_type
                        ),
                    ];
                    if let Some(plugin_source) = skill.plugin_source.as_deref() {
                        parts.push(format!("plugin_source={}", plugin_source));
                    }
                    if let Some(source_path) = skill.source_path.as_deref() {
                        parts.push(format!("source_path={}", source_path));
                    }
                    if let Some(description) = skill.description.as_deref() {
                        parts.push(format!(
                            "{}={}",
                            field(locale, "简介", "description"),
                            description
                        ));
                    }
                    if let Some(updated_at) = skill.updated_at.as_deref() {
                        parts.push(format!("updated_at={}", updated_at));
                    }
                    lines.push(format!("{}. {}", index + 1, parts.join(" | ")));
                    lines.push(text(locale, "内容：", "Content:"));
                    let content = skill.content.trim();
                    if content.is_empty() {
                        lines.push(text(locale, "（空）", "(empty)"));
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

            lines.push(text(
                locale,
                "所选技能关联插件全文：",
                "Full text of plugins related to selected skills:",
            ));
            if plugins.is_empty() {
                lines.push(text(locale, "无", "None"));
            } else {
                for (index, plugin) in plugins.iter().enumerate() {
                    let mut parts = vec![
                        format!("plugin_ref={}", plugin.plugin_ref),
                        format!("plugin_source={}", plugin.source),
                        format!("{}={}", field(locale, "名称", "name"), plugin.name),
                    ];
                    if let Some(category) = plugin.category.as_deref() {
                        parts.push(format!(
                            "{}={}",
                            field(locale, "分类", "category"),
                            category
                        ));
                    }
                    if let Some(description) = plugin.description.as_deref() {
                        parts.push(format!(
                            "{}={}",
                            field(locale, "简介", "description"),
                            description
                        ));
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
                        lines.push(text(locale, "插件内容：", "Plugin content:"));
                        for item in content.lines() {
                            lines.push(item.to_string());
                        }
                    }
                    if !plugin.commands.is_empty() {
                        lines.push(text(locale, "插件命令：", "Plugin commands:"));
                        for command in &plugin.commands {
                            let mut command_parts = vec![
                                format!(
                                    "{}={}",
                                    field(locale, "名称", "name"),
                                    command.name.trim()
                                ),
                                format!("source_path={}", command.source_path.trim()),
                            ];
                            if let Some(description) = command.description.as_deref().map(str::trim)
                            {
                                if !description.is_empty() {
                                    command_parts.push(format!(
                                        "{}={}",
                                        field(locale, "简介", "description"),
                                        description
                                    ));
                                }
                            }
                            if let Some(argument_hint) =
                                command.argument_hint.as_deref().map(str::trim)
                            {
                                if !argument_hint.is_empty() {
                                    command_parts.push(format!(
                                        "{}={}",
                                        field(locale, "参数提示", "argument_hint"),
                                        argument_hint
                                    ));
                                }
                            }
                            lines.push(format!("- {}", command_parts.join(" | ")));
                            let command_content = command.content.trim();
                            if !command_content.is_empty() {
                                lines.push(text(locale, "  内容：", "  Content:"));
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

    #[cfg(test)]
    if matches!(skill_mode, ContactSkillPromptMode::Summary { .. }) {
        lines.push(String::new());
        lines.push(text(
            locale,
            "关联命令（使用 command_ref）：",
            "Related commands (use command_ref):",
        ));
        if !agent.runtime_commands.is_empty() {
            for (index, command) in agent.runtime_commands.iter().enumerate() {
                let mut parts = vec![
                    format!("command_ref={}", command.command_ref.trim()),
                    format!("{}={}", field(locale, "名称", "name"), command.name.trim()),
                    format!("plugin_source={}", command.plugin_source.trim()),
                ];
                parts.push(format!(
                    "{}={}",
                    field(locale, "简介", "description"),
                    command
                        .description
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or(text_ref(locale, "未提供", "not provided"))
                ));
                if let Some(argument_hint) = command.argument_hint.as_deref().map(str::trim) {
                    if !argument_hint.is_empty() {
                        parts.push(format!(
                            "{}={}",
                            field(locale, "参数提示", "argument_hint"),
                            argument_hint
                        ));
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
            lines.push(text(locale, "无", "None"));
        }

        if !agent.runtime_commands.is_empty() {
            lines.push(String::new());
            lines.push(if locale.is_english() {
                format!(
                    "If you need the full content of a command, call builtin tool `{}` and pass only `command_ref` (for example `CMD1`).",
                    CONTACT_COMMAND_READER_TOOL_NAME
                )
            } else {
                format!(
                    "如果需要查看某个 command 的完整内容，请调用内置工具 `{}`，仅传 `command_ref`（如 `CMD1`）。",
                    CONTACT_COMMAND_READER_TOOL_NAME
                )
            });
        }
    }

    #[cfg(test)]
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
            lines.push(if locale.is_english() {
                format!(
                    "If you need the full content of a plugin, call builtin tool `{}` and pass only `plugin_ref` (for example `PL1`).",
                    CONTACT_PLUGIN_READER_TOOL_NAME
                )
            } else {
                format!(
                    "如果需要查看某个 plugin 的完整内容，请调用内置工具 `{}`，仅传 `plugin_ref`（如 `PL1`）。",
                    CONTACT_PLUGIN_READER_TOOL_NAME
                )
            });
        } else {
            lines.push(if locale.is_english() {
                format!(
                    "If you need the full content of a plugin, call builtin tool `{}` and pass only `plugin_ref` (for example {}).",
                    CONTACT_PLUGIN_READER_TOOL_NAME,
                    plugin_examples.join(", ")
                )
            } else {
                format!(
                    "如果需要查看某个 plugin 的完整内容，请调用内置工具 `{}`，仅传 `plugin_ref`（如 {}）。",
                    CONTACT_PLUGIN_READER_TOOL_NAME,
                    plugin_examples.join(", ")
                )
            });
        }
    }

    #[cfg(test)]
    if matches!(skill_mode, ContactSkillPromptMode::Summary { .. }) && !skill_entries.is_empty() {
        lines.push(String::new());
        lines.push(if locale.is_english() {
            format!(
                "If you need the full content of a skill, call builtin tool `{}` and pass only `skill_ref` (for example `SK1`).",
                CONTACT_SKILL_READER_TOOL_NAME
            )
        } else {
            format!(
                "如果需要查看某个 skill 的完整内容，请调用内置工具 `{}`，仅传 `skill_ref`（如 `SK1`）。",
                CONTACT_SKILL_READER_TOOL_NAME
            )
        });
    }

    Some(lines.join("\n").trim().to_string())
}

#[cfg(test)]
fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

fn text(locale: InternalContextLocale, zh: &'static str, en: &'static str) -> String {
    if locale.is_english() {
        en.to_string()
    } else {
        zh.to_string()
    }
}

#[cfg(test)]
fn text_ref(locale: InternalContextLocale, zh: &'static str, en: &'static str) -> &'static str {
    if locale.is_english() {
        en
    } else {
        zh
    }
}

#[cfg(test)]
fn field(locale: InternalContextLocale, zh: &'static str, en: &'static str) -> &'static str {
    if locale.is_english() {
        en
    } else {
        zh
    }
}

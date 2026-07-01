// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::internal_context_locale::InternalContextLocale;

use super::super::types::ParsedContactCommandInvocation;
use super::locale::{field, text};

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

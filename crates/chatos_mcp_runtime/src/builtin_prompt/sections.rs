// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::OnceLock;

use super::BuiltinMcpPromptLocale;

const BUILTIN_MCP_PROMPT_ZH_SOURCE: &str = include_str!("../../../../BUILTIN_MCP_PROMPT.zh-CN.md");
const BUILTIN_MCP_PROMPT_EN_SOURCE: &str = include_str!("../../../../BUILTIN_MCP_PROMPT.en-US.md");

static PROMPT_SECTIONS_ZH: OnceLock<HashMap<String, String>> = OnceLock::new();
static PROMPT_SECTIONS_EN: OnceLock<HashMap<String, String>> = OnceLock::new();

pub(super) fn prompt_sections(locale: BuiltinMcpPromptLocale) -> &'static HashMap<String, String> {
    if locale.is_english() {
        PROMPT_SECTIONS_EN.get_or_init(|| parse_prompt_sections(BUILTIN_MCP_PROMPT_EN_SOURCE))
    } else {
        PROMPT_SECTIONS_ZH.get_or_init(|| parse_prompt_sections(BUILTIN_MCP_PROMPT_ZH_SOURCE))
    }
}

fn parse_prompt_sections(markdown: &str) -> HashMap<String, String> {
    let mut sections = HashMap::new();
    let mut current_id: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in markdown.lines() {
        if let Some(section_id) = parse_section_header(line) {
            flush_section(&mut sections, &mut current_id, &mut current_lines);
            current_id = Some(section_id.to_string());
            continue;
        }

        if current_id.is_some() {
            current_lines.push(line);
        }
    }

    flush_section(&mut sections, &mut current_id, &mut current_lines);

    sections
}

fn flush_section(
    sections: &mut HashMap<String, String>,
    current_id: &mut Option<String>,
    current_lines: &mut Vec<&str>,
) {
    let Some(section_id) = current_id.take() else {
        current_lines.clear();
        return;
    };

    let content = current_lines.join("\n").trim().to_string();
    current_lines.clear();
    if !content.is_empty() {
        sections.insert(section_id, content);
    }
}

fn parse_section_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !(trimmed.starts_with("## [") && trimmed.ends_with(']')) {
        return None;
    }

    let inner = &trimmed[4..trimmed.len() - 1];
    let section_id = inner.trim();
    if section_id.is_empty() {
        None
    } else {
        Some(section_id)
    }
}

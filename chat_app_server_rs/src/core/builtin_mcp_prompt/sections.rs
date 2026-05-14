use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;

use crate::core::internal_context_locale::InternalContextLocale;
use crate::services::builtin_mcp::BuiltinMcpKind;

pub(super) const SECTION_GLOBAL: &str = "global";
pub(super) const SECTION_TASK_MANAGER: &str = "builtin_task_manager";
pub(super) const SECTION_UI_PROMPTER: &str = "builtin_ui_prompter";
pub(super) const SECTION_CODE_MAINTAINER_READ: &str = "builtin_code_maintainer_read";
pub(super) const SECTION_CODE_MAINTAINER_WRITE: &str = "builtin_code_maintainer_write";
pub(super) const SECTION_TERMINAL_CONTROLLER: &str = "builtin_terminal_controller";
pub(super) const SECTION_REMOTE_CONNECTION_CONTROLLER: &str =
    "builtin_remote_connection_controller";
pub(super) const SECTION_BROWSER_TOOLS: &str = "builtin_browser_tools";
pub(super) const SECTION_WEB_TOOLS: &str = "builtin_web_tools";
pub(super) const SECTION_NOTEPAD: &str = "builtin_notepad";
pub(super) const SECTION_CONDITIONAL_CONTACT_MEMORY_READERS: &str =
    "conditional_contact_memory_readers";
pub(super) const SECTION_RUNTIME_LIMITATIONS: &str = "runtime_limitations";

pub(super) const SECTION_ORDER: &[&str] = &[
    SECTION_GLOBAL,
    SECTION_TASK_MANAGER,
    SECTION_UI_PROMPTER,
    SECTION_CODE_MAINTAINER_READ,
    SECTION_CODE_MAINTAINER_WRITE,
    SECTION_TERMINAL_CONTROLLER,
    SECTION_REMOTE_CONNECTION_CONTROLLER,
    SECTION_BROWSER_TOOLS,
    SECTION_WEB_TOOLS,
    SECTION_NOTEPAD,
    SECTION_CONDITIONAL_CONTACT_MEMORY_READERS,
];

pub(super) const BUILTIN_MCP_PROMPT_ZH_SOURCE_PATH: &str = "BUILTIN_MCP_PROMPT.zh-CN.md";
pub(super) const BUILTIN_MCP_PROMPT_EN_SOURCE_PATH: &str = "BUILTIN_MCP_PROMPT.en-US.md";
pub(super) const BUILTIN_MCP_PROMPT_ZH_SOURCE: &str =
    include_str!("../../../../BUILTIN_MCP_PROMPT.zh-CN.md");
pub(super) const BUILTIN_MCP_PROMPT_EN_SOURCE: &str =
    include_str!("../../../../BUILTIN_MCP_PROMPT.en-US.md");

#[derive(Debug, Clone)]
pub(super) struct PromptSectionRegistry {
    pub(super) ordered_ids: Vec<String>,
    pub(super) sections: HashMap<String, String>,
}

pub(super) static PROMPT_SECTION_REGISTRY_ZH: Lazy<PromptSectionRegistry> =
    Lazy::new(|| parse_prompt_sections(BUILTIN_MCP_PROMPT_ZH_SOURCE));

pub(super) static PROMPT_SECTION_REGISTRY_EN: Lazy<PromptSectionRegistry> =
    Lazy::new(|| parse_prompt_sections(BUILTIN_MCP_PROMPT_EN_SOURCE));

pub(super) fn prompt_source_path(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        BUILTIN_MCP_PROMPT_EN_SOURCE_PATH
    } else {
        BUILTIN_MCP_PROMPT_ZH_SOURCE_PATH
    }
}

pub(super) fn prompt_section_registry(
    locale: InternalContextLocale,
) -> &'static PromptSectionRegistry {
    if locale.is_english() {
        &PROMPT_SECTION_REGISTRY_EN
    } else {
        &PROMPT_SECTION_REGISTRY_ZH
    }
}

pub(super) fn section_id_for_kind(kind: BuiltinMcpKind) -> Option<&'static str> {
    match kind {
        BuiltinMcpKind::CodeMaintainerRead => Some(SECTION_CODE_MAINTAINER_READ),
        BuiltinMcpKind::CodeMaintainerWrite => Some(SECTION_CODE_MAINTAINER_WRITE),
        BuiltinMcpKind::TerminalController => Some(SECTION_TERMINAL_CONTROLLER),
        BuiltinMcpKind::TaskManager => Some(SECTION_TASK_MANAGER),
        BuiltinMcpKind::Notepad => Some(SECTION_NOTEPAD),
        BuiltinMcpKind::UiPrompter => Some(SECTION_UI_PROMPTER),
        BuiltinMcpKind::RemoteConnectionController => Some(SECTION_REMOTE_CONNECTION_CONTROLLER),
        BuiltinMcpKind::WebTools => Some(SECTION_WEB_TOOLS),
        BuiltinMcpKind::BrowserTools => Some(SECTION_BROWSER_TOOLS),
        BuiltinMcpKind::MemorySkillReader
        | BuiltinMcpKind::MemoryCommandReader
        | BuiltinMcpKind::MemoryPluginReader => Some(SECTION_CONDITIONAL_CONTACT_MEMORY_READERS),
        BuiltinMcpKind::AgentBuilder => None,
    }
}

pub(super) fn ordered_section_ids(section_ids: &HashSet<&'static str>) -> Vec<String> {
    SECTION_ORDER
        .iter()
        .filter(|section_id| section_ids.contains(**section_id))
        .map(|section_id| (*section_id).to_string())
        .collect()
}

pub(super) fn ordered_difference(
    candidate_sections: &HashSet<&'static str>,
    selected_sections: &HashSet<&'static str>,
) -> Vec<String> {
    SECTION_ORDER
        .iter()
        .filter(|section_id| {
            **section_id != SECTION_GLOBAL
                && candidate_sections.contains(**section_id)
                && !selected_sections.contains(**section_id)
        })
        .map(|section_id| (*section_id).to_string())
        .collect()
}

pub(super) fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn parse_prompt_sections(markdown: &str) -> PromptSectionRegistry {
    let mut ordered_ids = Vec::new();
    let mut sections = HashMap::new();
    let mut current_id: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in markdown.lines() {
        if let Some(section_id) = parse_section_header(line) {
            flush_section(
                &mut ordered_ids,
                &mut sections,
                &mut current_id,
                &mut current_lines,
            );
            current_id = Some(section_id.to_string());
            continue;
        }

        if current_id.is_some() {
            current_lines.push(line);
        }
    }

    flush_section(
        &mut ordered_ids,
        &mut sections,
        &mut current_id,
        &mut current_lines,
    );

    PromptSectionRegistry {
        ordered_ids,
        sections,
    }
}

fn flush_section(
    ordered_ids: &mut Vec<String>,
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
        if !ordered_ids.iter().any(|item| item == &section_id) {
            ordered_ids.push(section_id.clone());
        }
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

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use crate::builtin_catalog::BuiltinMcpKind;

mod sections;

const SECTION_ASK_USER: &str = "builtin_ask_user";
const SECTION_CODE_MAINTAINER_READ: &str = "builtin_code_maintainer_read";
const SECTION_CODE_MAINTAINER_WRITE: &str = "builtin_code_maintainer_write";
const SECTION_TERMINAL_CONTROLLER: &str = "builtin_terminal_controller";
const SECTION_REMOTE_CONNECTION_CONTROLLER: &str = "builtin_remote_connection_controller";
const SECTION_NOTEPAD: &str = "builtin_notepad";
const SECTION_AGENT_BUILDER: &str = "builtin_agent_builder";
const SECTION_CONTACT_MEMORY_READERS: &str = "conditional_contact_memory_readers";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BuiltinMcpPromptLocale {
    #[default]
    ZhCn,
    EnUs,
}

impl BuiltinMcpPromptLocale {
    pub const DEFAULT_KEY: &'static str = "zh-CN";
    pub const ENGLISH_KEY: &'static str = "en-US";

    pub fn from_key(value: Option<&str>) -> Self {
        match value
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .unwrap_or(Self::DEFAULT_KEY)
        {
            Self::ENGLISH_KEY => Self::EnUs,
            _ => Self::ZhCn,
        }
    }

    pub fn is_english(self) -> bool {
        matches!(self, Self::EnUs)
    }
}

pub fn builtin_mcp_provider_skill_instructions(
    kind: BuiltinMcpKind,
    locale: BuiltinMcpPromptLocale,
) -> Option<String> {
    let section_id = match kind {
        BuiltinMcpKind::CodeMaintainerRead => SECTION_CODE_MAINTAINER_READ,
        BuiltinMcpKind::CodeMaintainerWrite => SECTION_CODE_MAINTAINER_WRITE,
        BuiltinMcpKind::TerminalController => SECTION_TERMINAL_CONTROLLER,
        BuiltinMcpKind::Notepad => SECTION_NOTEPAD,
        BuiltinMcpKind::AskUser => SECTION_ASK_USER,
        BuiltinMcpKind::RemoteConnectionController => SECTION_REMOTE_CONNECTION_CONTROLLER,
        BuiltinMcpKind::MemorySkillReader
        | BuiltinMcpKind::MemoryCommandReader
        | BuiltinMcpKind::MemoryPluginReader => SECTION_CONTACT_MEMORY_READERS,
        BuiltinMcpKind::AgentBuilder => SECTION_AGENT_BUILDER,
    };
    sections::prompt_sections(locale)
        .get(section_id)
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_skill_instructions_keep_both_locales() {
        let zh = builtin_mcp_provider_skill_instructions(
            BuiltinMcpKind::AskUser,
            BuiltinMcpPromptLocale::ZhCn,
        )
        .unwrap();
        let en = builtin_mcp_provider_skill_instructions(
            BuiltinMcpKind::AskUser,
            BuiltinMcpPromptLocale::EnUs,
        )
        .unwrap();

        assert!(zh.contains("ask_user_prompt_choices"));
        assert!(en.contains("ask_user_prompt_choices"));
    }
}

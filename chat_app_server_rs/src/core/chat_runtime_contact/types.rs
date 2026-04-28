use crate::services::memory_server_client::MemorySkillPluginCommandDto;

pub const CONTACT_SKILL_READER_TOOL_NAME: &str = "memory_skill_reader_get_skill_detail";
pub const CONTACT_COMMAND_READER_TOOL_NAME: &str = "memory_command_reader_get_command_detail";
pub const CONTACT_PLUGIN_READER_TOOL_NAME: &str = "memory_plugin_reader_get_plugin_detail";

#[derive(Debug, Clone)]
pub struct ParsedContactCommandInvocation {
    pub command_ref: String,
    pub name: String,
    pub plugin_source: String,
    pub source_path: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub content: String,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedImplicitCommandSelection {
    pub command_ref: Option<String>,
    pub name: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
}

#[derive(Debug, Clone)]
pub struct ContactSelectedSkillPrompt {
    pub skill_ref: String,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub plugin_source: Option<String>,
    pub source_path: Option<String>,
    pub source_type: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContactSelectedPluginPrompt {
    pub plugin_ref: String,
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub content: Option<String>,
    pub commands: Vec<MemorySkillPluginCommandDto>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ContactSkillPromptMode {
    Disabled,
    Summary {
        force_skill_first: bool,
    },
    SelectedFull {
        skills: Vec<ContactSelectedSkillPrompt>,
        plugins: Vec<ContactSelectedPluginPrompt>,
    },
}

pub fn contact_skill_ref(index: usize) -> String {
    format!("SK{}", index + 1)
}

pub fn contact_plugin_ref(index: usize) -> String {
    format!("PL{}", index + 1)
}

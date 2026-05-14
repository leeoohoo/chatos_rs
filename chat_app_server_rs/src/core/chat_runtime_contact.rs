#[path = "chat_runtime_contact/command_parser.rs"]
mod command_parser;
#[path = "chat_runtime_contact/prompt_builder.rs"]
mod prompt_builder;
#[path = "chat_runtime_contact/types.rs"]
mod types;

pub use self::command_parser::{
    parse_contact_command_invocation, parse_implicit_command_selections_from_tools_end,
};
pub use self::prompt_builder::{
    compose_contact_command_system_prompt, compose_contact_system_prompt,
};
pub use self::types::{
    contact_plugin_ref, contact_skill_ref, ContactSelectedPluginPrompt, ContactSelectedSkillPrompt,
    ContactSkillPromptMode, ParsedContactCommandInvocation, ParsedImplicitCommandSelection,
    CONTACT_COMMAND_READER_TOOL_NAME, CONTACT_PLUGIN_READER_TOOL_NAME,
    CONTACT_SKILL_READER_TOOL_NAME,
};

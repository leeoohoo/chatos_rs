#[path = "analyzer.rs"]
mod analyzer;
#[path = "dispatcher.rs"]
mod dispatcher;

const SHELL_BUILTINS: &[&str] = &[
    "cd", "export", "unset", "alias", "unalias", "source", ".", "echo", "printf", "test", "[",
];

#[derive(Debug, Clone)]
pub struct RunDispatchResult {
    pub terminal_id: String,
    pub terminal_name: String,
    pub terminal_reused: bool,
    pub cwd: String,
    pub executed_command: String,
}

#[derive(Debug, Clone)]
pub struct RunExecutionInput {
    pub target_id: Option<String>,
    pub cwd: Option<String>,
    pub command: Option<String>,
    pub create_if_missing: bool,
}

pub(crate) use self::analyzer::{analyze_project, apply_default_target};
pub(crate) use self::dispatcher::{
    dispatch_command, resolve_execution, validate_command_preflight,
};

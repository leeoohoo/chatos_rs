#[path = "analyzer.rs"]
mod analyzer;
#[path = "cache.rs"]
mod cache;
#[path = "dispatcher.rs"]
mod dispatcher;
#[path = "environment.rs"]
mod environment;

const SHELL_BUILTINS: &[&str] = &[
    "cd", "export", "unset", "alias", "unalias", "source", ".", "echo", "printf", "test", "[",
];

#[derive(Debug, Clone)]
pub struct RunDispatchResult {
    pub terminal_id: String,
    pub terminal_name: String,
    pub terminal_reused: bool,
    pub terminal_status: String,
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

pub(crate) use self::analyzer::{
    analyze_project, apply_default_target, classify_project_run_path_change,
    ProjectRunPathChangeKind,
};
pub(crate) use self::dispatcher::{
    dispatch_command, resolve_execution, validate_command_preflight,
};
pub(crate) use self::cache::{
    clear_cached_environment_snapshot, read_cached_catalog, write_cached_catalog,
};
pub(crate) use self::environment::{
    env_overrides_for_target, load_environment_selection, load_environment_snapshot,
    refresh_environment_snapshot,
    resolve_command_with_toolchains, save_environment_selection,
    validate_project_run_target,
};

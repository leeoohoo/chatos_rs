// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "analyzer.rs"]
mod analyzer;
#[path = "cache.rs"]
mod cache;
#[path = "dispatcher.rs"]
mod dispatcher;
#[path = "environment.rs"]
mod environment;
#[path = "environment_discovery.rs"]
mod environment_discovery;
#[path = "environment_runtime.rs"]
mod environment_runtime;
#[path = "environment_support.rs"]
mod environment_support;
#[path = "environment_validation.rs"]
mod environment_validation;
#[path = "file_limits.rs"]
mod file_limits;

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
pub(crate) use self::cache::{
    clear_cached_environment_snapshot, read_cached_catalog, write_cached_catalog,
};
pub(crate) use self::dispatcher::{
    dispatch_command, resolve_execution, validate_command_preflight,
};
pub(crate) use self::environment::{
    load_environment_selection, load_environment_snapshot, refresh_environment_snapshot,
    save_environment_selection,
};
pub(crate) use self::environment_runtime::{
    env_overrides_for_target, resolve_command_with_toolchains,
};
pub(crate) use self::environment_validation::validate_project_run_target;

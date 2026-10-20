use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunAnalysisSnapshot {
    pub(crate) catalog: ProjectRunCatalog,
    pub(crate) environment: ProjectRunEnvironment,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunCatalog {
    pub(crate) project_id: String,
    pub(crate) status: &'static str,
    pub(crate) default_target_id: Option<String>,
    pub(crate) targets: Vec<ProjectRunTarget>,
    pub(crate) error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunTarget {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) language: Option<String>,
    pub(crate) cwd: String,
    pub(crate) command: String,
    pub(crate) source: String,
    pub(crate) is_default: bool,
    pub(crate) entrypoint: Option<String>,
    pub(crate) manifest_path: Option<String>,
    pub(crate) required_toolchains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunEnvironment {
    pub(crate) toolchain_options: BTreeMap<String, Vec<ProjectRunToolchainOption>>,
    pub(crate) configuration_files: Vec<ProjectRunConfigurationFile>,
    pub(crate) validation_issues: Vec<ProjectRunValidationIssue>,
    pub(crate) selected_toolchains: BTreeMap<String, String>,
    pub(crate) custom_toolchains: BTreeMap<String, ProjectRunCustomToolchain>,
    pub(crate) environment_variables: Vec<ProjectRunEnvironmentVariable>,
    pub(crate) terminal_ui_enabled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunToolchainOption {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) version: Option<String>,
    pub(crate) source: &'static str,
    pub(crate) is_default: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunConfigurationFile {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) preview: Option<String>,
    pub(crate) source: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunValidationIssue {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) message: String,
    pub(crate) target_id: Option<String>,
    pub(crate) target_label: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunCustomToolchain {
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) executable_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRunEnvironmentVariable {
    pub(crate) key: String,
    pub(crate) has_value: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct AnalyzedProjectRunTarget {
    pub(crate) presentation: ProjectRunTarget,
    pub(crate) invocation: ProjectRunInvocation,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum ProjectRunExecutable {
    Toolchain(String),
    ProjectFile(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ProjectRunInvocation {
    pub(crate) executable: ProjectRunExecutable,
    pub(crate) arguments: Vec<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct DiscoveredToolchain {
    pub(crate) presentation: ProjectRunToolchainOption,
    pub(crate) executable: PathBuf,
}

#[derive(Debug)]
pub(crate) struct ProjectRunAnalysis {
    pub(crate) targets: Vec<AnalyzedProjectRunTarget>,
    pub(crate) configuration_files: Vec<ProjectRunConfigurationFile>,
    pub(crate) toolchains: BTreeMap<String, Vec<DiscoveredToolchain>>,
}

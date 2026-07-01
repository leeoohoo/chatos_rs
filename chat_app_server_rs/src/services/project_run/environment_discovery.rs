// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::models::project::Project;
use crate::models::project_run_environment::{
    ProjectRunEnvironmentSelection, ProjectRunToolchainOption,
};

use super::environment_support::{
    home_dir, infer_version_suffix, normalize_string, normalized_selected_toolchain_id,
    resolve_user_path,
};

#[path = "environment_discovery/config_files.rs"]
mod config_files;
#[path = "environment_discovery/hints.rs"]
mod hints;
#[path = "environment_discovery/project_toolchains.rs"]
mod project_toolchains;
#[path = "environment_discovery/support.rs"]
mod support;
#[path = "environment_discovery/system_toolchains.rs"]
mod system_toolchains;

pub(super) use config_files::collect_project_config_files;

use hints::collect_project_toolchain_hints;
use project_toolchains::discover_project_local_toolchains;
use support::{push_option_with_label, ToolchainOptions, ToolchainSeen};
use system_toolchains::{
    discover_gradle_user_homes, discover_homebrew_bins, discover_java_homes,
    discover_known_commands, discover_maven_settings, discover_user_versioned_bins,
};

fn option_matches_hint(option: &ProjectRunToolchainOption, hints: &[String]) -> bool {
    if hints.is_empty() {
        return false;
    }
    let blob = format!(
        "{} {} {}",
        option.label.to_lowercase(),
        option.path.to_lowercase(),
        option.version.clone().unwrap_or_default().to_lowercase()
    );
    hints
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .any(|value| blob.contains(value.as_str()))
}

fn source_priority(source: &str) -> usize {
    match source {
        "sandbox" => 0,
        "project-local" => 1,
        "env" => 2,
        "path" => 3,
        "system" => 4,
        "manual" => 5,
        _ => 6,
    }
}

pub(super) fn discover_toolchain_options(
    project: &Project,
    selection: Option<&ProjectRunEnvironmentSelection>,
) -> HashMap<String, Vec<ProjectRunToolchainOption>> {
    let mut grouped = ToolchainOptions::new();
    let mut seen = ToolchainSeen::new();
    let project_root = PathBuf::from(resolve_user_path(project.root_path.as_str()));
    let hints = collect_project_toolchain_hints(project_root.as_path());

    if project_root.is_dir() {
        discover_project_local_toolchains(&mut grouped, &mut seen, project_root.as_path());
        discover_maven_settings(&mut grouped, &mut seen);
        discover_gradle_user_homes(&mut grouped, &mut seen);
    }

    discover_java_homes(&mut grouped, &mut seen);
    discover_known_commands(&mut grouped, &mut seen, "java", &["java"]);
    discover_known_commands(&mut grouped, &mut seen, "mvn", &["mvn", "mvn.cmd"]);
    discover_known_commands(&mut grouped, &mut seen, "gradle", &["gradle"]);
    discover_known_commands(&mut grouped, &mut seen, "cargo", &["cargo"]);
    discover_known_commands(&mut grouped, &mut seen, "rustc", &["rustc"]);
    discover_known_commands(&mut grouped, &mut seen, "go", &["go"]);
    discover_known_commands(&mut grouped, &mut seen, "node", &["node"]);
    discover_known_commands(&mut grouped, &mut seen, "npm", &["npm"]);
    discover_known_commands(&mut grouped, &mut seen, "pnpm", &["pnpm"]);
    discover_known_commands(&mut grouped, &mut seen, "yarn", &["yarn"]);
    discover_known_commands(&mut grouped, &mut seen, "python", &["python", "python3"]);

    if let Some(home) = home_dir() {
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "mvn",
            &[format!("{home}/.sdkman/candidates/maven")],
            &["mvn"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "gradle",
            &[format!("{home}/.sdkman/candidates/gradle")],
            &["gradle"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "cargo",
            &[
                format!("{home}/.asdf/installs/rust"),
                format!("{home}/.rustup/toolchains"),
            ],
            &["cargo"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "rustc",
            &[
                format!("{home}/.asdf/installs/rust"),
                format!("{home}/.rustup/toolchains"),
            ],
            &["rustc"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "go",
            &[
                format!("{home}/.asdf/installs/golang"),
                format!("{home}/.gvm/gos"),
            ],
            &["go"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "node",
            &[
                format!("{home}/.nvm/versions/node"),
                format!("{home}/.asdf/installs/nodejs"),
                format!("{home}/.volta/tools/image/node"),
            ],
            &["node"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "npm",
            &[
                format!("{home}/.nvm/versions/node"),
                format!("{home}/.asdf/installs/nodejs"),
                format!("{home}/.volta/tools/image/node"),
            ],
            &["npm"],
        );
        discover_user_versioned_bins(
            &mut grouped,
            &mut seen,
            "python",
            &[
                format!("{home}/.pyenv/versions"),
                format!("{home}/miniconda3/envs"),
                format!("{home}/anaconda3/envs"),
                format!("{home}/opt/anaconda3/envs"),
                format!("{home}/.asdf/installs/python"),
            ],
            &["python", "python3"],
        );
    }

    discover_homebrew_bins(&mut grouped, &mut seen, "mvn", &["maven"], &["mvn"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "gradle", &["gradle"], &["gradle"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "cargo", &["rust"], &["cargo"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "rustc", &["rust"], &["rustc"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "go", &["go"], &["go"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "node", &["node"], &["node"]);
    discover_homebrew_bins(&mut grouped, &mut seen, "npm", &["node"], &["npm"]);
    discover_homebrew_bins(
        &mut grouped,
        &mut seen,
        "python",
        &["python"],
        &["python3", "python"],
    );

    if let Some(selection) = selection {
        for (map_kind, custom) in &selection.custom_toolchains {
            let kind = normalize_string(custom.kind.as_str());
            let key_kind = if kind.is_empty() {
                normalize_string(map_kind.as_str())
            } else {
                kind
            };
            if key_kind.is_empty() {
                continue;
            }
            let path = resolve_user_path(custom.path.as_str());
            if path.is_empty() {
                continue;
            }
            let label = normalize_string(custom.label.as_str());
            push_option_with_label(
                &mut grouped,
                &mut seen,
                key_kind.as_str(),
                path,
                "manual",
                None,
                Some(if label.is_empty() {
                    format!(
                        "手动指定: {}",
                        infer_version_suffix(Path::new(custom.path.as_str()))
                    )
                } else {
                    label
                }),
                true,
            );
        }
    }

    let selected_ids = selection
        .map(|value| {
            value
                .selected_toolchains
                .iter()
                .map(|(kind, id)| {
                    (
                        normalize_string(kind.as_str()),
                        normalized_selected_toolchain_id(
                            kind.as_str(),
                            id.as_str(),
                            &value.custom_toolchains,
                        ),
                    )
                })
                .filter(|(kind, id)| !kind.is_empty() && !id.is_empty())
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    for (kind, rows) in grouped.iter_mut() {
        let selected_id = selected_ids.get(kind);
        let hint_tokens = hints.tokens_by_kind.get(kind).cloned().unwrap_or_default();
        rows.sort_by(|left, right| {
            let left_selected = selected_id.is_some_and(|value| value == &left.id);
            let right_selected = selected_id.is_some_and(|value| value == &right.id);
            let left_hint = option_matches_hint(left, hint_tokens.as_slice());
            let right_hint = option_matches_hint(right, hint_tokens.as_slice());

            right_selected
                .cmp(&left_selected)
                .then_with(|| right_hint.cmp(&left_hint))
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| {
                    source_priority(left.source.as_str())
                        .cmp(&source_priority(right.source.as_str()))
                })
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.path.cmp(&right.path))
        });
    }

    grouped.into_iter().collect()
}

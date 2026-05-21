use std::collections::HashMap;
use std::path::Path;

use crate::models::project_run::ProjectRunTarget;
use crate::models::project_run_environment::{
    ProjectRunEnvironmentSelection, ProjectRunToolchainOption,
};

use super::environment_support::{
    infer_required_toolchains, normalize_path, prepend_path_entry, selected_or_first_option,
    shell_quote_path,
};

pub(crate) fn env_overrides_for_target(
    target: &ProjectRunTarget,
    selection: Option<&ProjectRunEnvironmentSelection>,
    options_by_kind: &HashMap<String, Vec<ProjectRunToolchainOption>>,
) -> HashMap<String, String> {
    let mut env = selection.map(|value| value.env_vars.clone()).unwrap_or_default();

    for kind in infer_required_toolchains(target) {
        let Some(option) = selected_or_first_option(kind.as_str(), selection, options_by_kind) else {
            continue;
        };

        match kind.as_str() {
            "java_home" => {
                env.insert("JAVA_HOME".to_string(), option.path.clone());
                let java_bin = Path::new(option.path.as_str()).join("bin");
                prepend_path_entry(&mut env, normalize_path(java_bin.as_path()).as_str());
            }
            "python" => {
                env.insert("PYTHON_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                    if let Some(env_root) = bin_dir.parent() {
                        env.insert("VIRTUAL_ENV".to_string(), normalize_path(env_root));
                    }
                }
            }
            "node" => {
                env.insert("NODE_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "mvn" => {
                env.insert("MVN_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "gradle" => {
                env.insert("GRADLE_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "gradle_user_home" => {
                env.insert("GRADLE_USER_HOME".to_string(), option.path.clone());
            }
            "cargo" => {
                env.insert("CARGO_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            "go" => {
                env.insert("GO_BIN".to_string(), option.path.clone());
                if let Some(bin_dir) = Path::new(option.path.as_str()).parent() {
                    prepend_path_entry(&mut env, normalize_path(bin_dir).as_str());
                }
            }
            _ => {}
        }
    }

    env
}

fn rewrite_command_prefix(command: String, prefix: &str, replacement_path: &str) -> String {
    if command == prefix || command.starts_with(&format!("{prefix} ")) {
        return command.replacen(prefix, shell_quote_path(replacement_path).as_str(), 1);
    }
    command
}

fn inject_maven_settings_arg(command: String, settings_path: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() || settings_path.trim().is_empty() {
        return command;
    }
    if trimmed.contains(" -s ") || trimmed.contains(" --settings ") {
        return command;
    }
    if !(trimmed == "mvn"
        || trimmed.starts_with("mvn ")
        || trimmed == "./mvnw"
        || trimmed.starts_with("./mvnw "))
    {
        return command;
    }

    let mut parts = trimmed.splitn(2, ' ');
    let prefix = parts.next().unwrap_or(trimmed);
    let rest = parts.next().unwrap_or("").trim();
    if rest.is_empty() {
        format!("{prefix} -s {}", shell_quote_path(settings_path))
    } else {
        format!("{prefix} -s {} {rest}", shell_quote_path(settings_path))
    }
}

pub(crate) fn resolve_command_with_toolchains(
    target: &ProjectRunTarget,
    selection: Option<&ProjectRunEnvironmentSelection>,
    options_by_kind: &HashMap<String, Vec<ProjectRunToolchainOption>>,
) -> String {
    let mut command = target.command.clone();

    let resolve_binary = |kind: &str| -> Option<String> {
        selected_or_first_option(kind, selection, options_by_kind).map(|item| item.path.clone())
    };

    if let Some(bin) = resolve_binary("java") {
        command = rewrite_command_prefix(command, "java", bin.as_str());
    }
    if let Some(java_home) = resolve_binary("java_home") {
        let java_bin = Path::new(java_home.as_str()).join("bin").join("java");
        if java_bin.is_file() {
            command =
                rewrite_command_prefix(command, "java", normalize_path(java_bin.as_path()).as_str());
        }
    }
    if let Some(bin) = resolve_binary("mvn") {
        command = rewrite_command_prefix(command, "mvn", bin.as_str());
    }
    if let Some(bin) = resolve_binary("gradle") {
        command = rewrite_command_prefix(command, "gradle", bin.as_str());
    }
    if let Some(bin) = resolve_binary("cargo") {
        command = rewrite_command_prefix(command, "cargo", bin.as_str());
    }
    if let Some(bin) = resolve_binary("rustc") {
        command = rewrite_command_prefix(command, "rustc", bin.as_str());
    }
    if let Some(bin) = resolve_binary("go") {
        command = rewrite_command_prefix(command, "go", bin.as_str());
    }
    if let Some(bin) = resolve_binary("node") {
        command = rewrite_command_prefix(command, "node", bin.as_str());
    }
    if let Some(bin) = resolve_binary("npm") {
        command = rewrite_command_prefix(command, "npm", bin.as_str());
    }
    if let Some(bin) = resolve_binary("pnpm") {
        command = rewrite_command_prefix(command, "pnpm", bin.as_str());
    }
    if let Some(bin) = resolve_binary("yarn") {
        command = rewrite_command_prefix(command, "yarn", bin.as_str());
    }
    if let Some(bin) = resolve_binary("python") {
        command = rewrite_command_prefix(command, "python", bin.as_str());
        command = rewrite_command_prefix(command, "python3", bin.as_str());
        if let Some(bin_dir) = Path::new(bin.as_str()).parent() {
            let pytest = bin_dir.join("pytest");
            if pytest.is_file() {
                command = rewrite_command_prefix(
                    command,
                    "pytest",
                    normalize_path(pytest.as_path()).as_str(),
                );
            }
        }
    }
    if let Some(settings) = resolve_binary("mvn_settings") {
        command = inject_maven_settings_arg(command, settings.as_str());
    }

    command
}

#[cfg(test)]
mod tests {
    use super::{inject_maven_settings_arg, rewrite_command_prefix};

    #[test]
    fn rewrite_command_prefix_quotes_paths_with_spaces() {
        let rewritten = rewrite_command_prefix(
            "python script.py".to_string(),
            "python",
            "/tmp/my env/bin/python",
        );
        assert_eq!(rewritten, "'/tmp/my env/bin/python' script.py");
    }

    #[test]
    fn inject_maven_settings_arg_adds_flag_once() {
        let updated = inject_maven_settings_arg(
            "mvn test".to_string(),
            "/tmp/settings.xml",
        );
        assert_eq!(updated, "mvn -s /tmp/settings.xml test");

        let unchanged = inject_maven_settings_arg(
            "mvn -s /tmp/settings.xml test".to_string(),
            "/tmp/other.xml",
        );
        assert_eq!(unchanged, "mvn -s /tmp/settings.xml test");
    }
}

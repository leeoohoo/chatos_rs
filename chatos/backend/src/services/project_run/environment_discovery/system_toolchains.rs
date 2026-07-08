// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use super::super::environment_support::{home_dir, normalize_path, option_id, resolve_user_path};
use super::support::{
    discover_direct_file_option, java_home_candidate, list_child_dirs, push_if_exists, push_option,
    push_option_with_label, ToolchainOptions, ToolchainSeen,
};

fn command_exists(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for segment in std::env::split_paths(&path_var) {
        let candidate = segment.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(super) fn discover_java_homes(out: &mut ToolchainOptions, seen: &mut ToolchainSeen) {
    let preferred_id = std::env::var("JAVA_HOME")
        .ok()
        .map(|value| resolve_user_path(value.as_str()))
        .filter(|value| !value.is_empty())
        .map(|path| option_id("java_home", path.as_str()));

    if let Some(java_home) = preferred_id
        .as_ref()
        .and_then(|_| std::env::var("JAVA_HOME").ok())
        .map(|value| resolve_user_path(value.as_str()))
        .filter(|value| !value.is_empty())
    {
        push_option_with_label(
            out,
            seen,
            "java_home",
            java_home,
            "env",
            preferred_id.as_deref(),
            Some("JAVA_HOME".to_string()),
            false,
        );
    }

    let mut roots = vec![
        "/Library/Java/JavaVirtualMachines".to_string(),
        "/usr/lib/jvm".to_string(),
        "/opt/homebrew/opt".to_string(),
        "/usr/local/opt".to_string(),
        "/opt/homebrew/Cellar".to_string(),
        "/usr/local/Cellar".to_string(),
    ];
    if let Some(home) = home_dir() {
        roots.push(format!("{home}/.sdkman/candidates/java"));
        roots.push(format!("{home}/.asdf/installs/java"));
        roots.push(format!("{home}/.jenv/versions"));
    }

    for raw_root in roots {
        let root = PathBuf::from(raw_root);
        if let Some(candidate) = java_home_candidate(root.as_path()) {
            push_option(
                out,
                seen,
                "java_home",
                candidate,
                "system",
                preferred_id.as_deref(),
            );
        }
        for child in list_child_dirs(root.as_path()) {
            if let Some(candidate) = java_home_candidate(child.as_path()) {
                push_option(
                    out,
                    seen,
                    "java_home",
                    candidate,
                    "system",
                    preferred_id.as_deref(),
                );
                continue;
            }
            for grandchild in list_child_dirs(child.as_path()) {
                if let Some(candidate) = java_home_candidate(grandchild.as_path()) {
                    push_option(
                        out,
                        seen,
                        "java_home",
                        candidate,
                        "system",
                        preferred_id.as_deref(),
                    );
                }
            }
        }
    }
}

pub(super) fn discover_known_commands(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    commands: &[&str],
) {
    let preferred_id = commands
        .iter()
        .find_map(|command| command_exists(command))
        .map(|path| option_id(kind, normalize_path(path.as_path()).as_str()));

    for command in commands {
        if let Some(path) = command_exists(command) {
            push_option(out, seen, kind, path, "path", preferred_id.as_deref());
        }
    }
}

fn discover_versioned_bin_root(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    root: &Path,
    binary_names: &[&str],
) {
    for version_dir in list_child_dirs(root) {
        for binary_name in binary_names {
            let candidate = version_dir.join("bin").join(binary_name);
            push_if_exists(out, seen, kind, candidate, "system", None);
        }
    }
}

pub(super) fn discover_homebrew_bins(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    package_prefixes: &[&str],
    binary_names: &[&str],
) {
    for cellar_root in ["/opt/homebrew/Cellar", "/usr/local/Cellar"] {
        let root = Path::new(cellar_root);
        if !root.is_dir() {
            continue;
        }
        for package_dir in list_child_dirs(root) {
            let Some(name) = package_dir.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !package_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                continue;
            }
            discover_versioned_bin_root(out, seen, kind, package_dir.as_path(), binary_names);
        }
    }
    for opt_root in ["/opt/homebrew/opt", "/usr/local/opt"] {
        let root = Path::new(opt_root);
        if !root.is_dir() {
            continue;
        }
        for package_dir in list_child_dirs(root) {
            let Some(name) = package_dir.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !package_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                continue;
            }
            for binary_name in binary_names {
                push_if_exists(
                    out,
                    seen,
                    kind,
                    package_dir.join("bin").join(binary_name),
                    "system",
                    None,
                );
            }
        }
    }
}

pub(super) fn discover_user_versioned_bins(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    roots: &[String],
    binary_names: &[&str],
) {
    for root in roots {
        discover_versioned_bin_root(out, seen, kind, Path::new(root), binary_names);
    }
}

pub(super) fn discover_maven_settings(out: &mut ToolchainOptions, seen: &mut ToolchainSeen) {
    if let Some(home) = home_dir() {
        discover_direct_file_option(
            out,
            seen,
            "mvn_settings",
            Path::new(home.as_str()).join(".m2/settings.xml").as_path(),
            "env",
            "用户 Maven 设置: ~/.m2/settings.xml",
        );
        discover_direct_file_option(
            out,
            seen,
            "mvn_settings",
            Path::new(home.as_str())
                .join(".config/maven/settings.xml")
                .as_path(),
            "system",
            "用户 Maven 设置: ~/.config/maven/settings.xml",
        );
    }

    for candidate in [
        "/usr/local/etc/maven/settings.xml",
        "/opt/homebrew/etc/maven/settings.xml",
        "/etc/maven/settings.xml",
        "/etc/maven/settings-user.xml",
    ] {
        discover_direct_file_option(
            out,
            seen,
            "mvn_settings",
            Path::new(candidate),
            "system",
            candidate,
        );
    }
}

pub(super) fn discover_gradle_user_homes(out: &mut ToolchainOptions, seen: &mut ToolchainSeen) {
    if let Some(home) = home_dir() {
        let gradle_home = Path::new(home.as_str()).join(".gradle");
        if gradle_home.is_dir() {
            push_option_with_label(
                out,
                seen,
                "gradle_user_home",
                normalize_path(gradle_home.as_path()),
                "env",
                None,
                Some("用户 Gradle Home: ~/.gradle".to_string()),
                false,
            );
        }

        let gradle_config_home = Path::new(home.as_str()).join(".config/gradle");
        if gradle_config_home.is_dir() {
            push_option_with_label(
                out,
                seen,
                "gradle_user_home",
                normalize_path(gradle_config_home.as_path()),
                "system",
                None,
                Some("用户 Gradle Home: ~/.config/gradle".to_string()),
                false,
            );
        }
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use super::super::environment_support::normalize_path;
use super::support::{
    discover_direct_file_option, java_home_candidate, list_child_dirs, push_option_with_label,
    push_relative_option, ToolchainOptions, ToolchainSeen,
};

fn discover_project_local_java_homes(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    project_root: &Path,
) {
    let direct_candidates = [".jdk", ".java", ".java_home", "jdk"];
    for candidate in direct_candidates {
        let path = project_root.join(candidate);
        if let Some(home) = java_home_candidate(path.as_path()) {
            push_option_with_label(
                out,
                seen,
                "java_home",
                normalize_path(home.as_path()),
                "project-local",
                None,
                Some(format!("项目内 JDK: {}", candidate)),
                false,
            );
        }
    }

    for dir_name in [".jdks", "jdks", ".toolchains/java", ".toolchains/jdks"] {
        let root = project_root.join(dir_name);
        for child in list_child_dirs(root.as_path()) {
            if let Some(home) = java_home_candidate(child.as_path()) {
                push_option_with_label(
                    out,
                    seen,
                    "java_home",
                    normalize_path(home.as_path()),
                    "project-local",
                    None,
                    Some(format!(
                        "项目内 JDK: {}",
                        child
                            .file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or(dir_name)
                    )),
                    false,
                );
                continue;
            }
            for grandchild in list_child_dirs(child.as_path()) {
                if let Some(home) = java_home_candidate(grandchild.as_path()) {
                    push_option_with_label(
                        out,
                        seen,
                        "java_home",
                        normalize_path(home.as_path()),
                        "project-local",
                        None,
                        Some(format!(
                            "项目内 JDK: {}",
                            grandchild
                                .file_name()
                                .and_then(|value| value.to_str())
                                .unwrap_or(dir_name)
                        )),
                        false,
                    );
                }
            }
        }
    }
}

pub(super) fn discover_project_local_toolchains(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    project_root: &Path,
) {
    discover_project_local_java_homes(out, seen, project_root);

    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".venv/bin/python",
        "sandbox",
        "项目虚拟环境: .venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".venv/bin/python3",
        "sandbox",
        "项目虚拟环境: .venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        "venv/bin/python",
        "sandbox",
        "项目虚拟环境: venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        "venv/bin/python3",
        "sandbox",
        "项目虚拟环境: venv",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        "env/bin/python",
        "sandbox",
        "项目虚拟环境: env",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".conda/bin/python",
        "sandbox",
        "项目 Conda 环境",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".pixi/envs/default/bin/python",
        "sandbox",
        "项目 Pixi 环境",
    );
    push_relative_option(
        out,
        seen,
        "python",
        project_root,
        ".pixi/envs/default/bin/python3",
        "sandbox",
        "项目 Pixi 环境",
    );

    push_relative_option(
        out,
        seen,
        "node",
        project_root,
        ".node/bin/node",
        "project-local",
        "项目内 Node",
    );
    push_relative_option(
        out,
        seen,
        "node",
        project_root,
        ".nodeenv/bin/node",
        "sandbox",
        "项目 Node 环境",
    );
    push_relative_option(
        out,
        seen,
        "cargo",
        project_root,
        ".cargo/bin/cargo",
        "project-local",
        "项目内 Cargo",
    );
    push_relative_option(
        out,
        seen,
        "rustc",
        project_root,
        ".cargo/bin/rustc",
        "project-local",
        "项目内 Rustc",
    );
    push_relative_option(
        out,
        seen,
        "go",
        project_root,
        ".go/bin/go",
        "project-local",
        "项目内 Go",
    );
    push_relative_option(
        out,
        seen,
        "mvn",
        project_root,
        "mvnw",
        "project-local",
        "项目 Wrapper: mvnw",
    );
    push_relative_option(
        out,
        seen,
        "gradle",
        project_root,
        "gradlew",
        "project-local",
        "项目 Wrapper: gradlew",
    );

    discover_direct_file_option(
        out,
        seen,
        "mvn_settings",
        project_root.join(".mvn/settings.xml").as_path(),
        "project-local",
        "项目 Maven 设置: .mvn/settings.xml",
    );
    discover_direct_file_option(
        out,
        seen,
        "mvn_settings",
        project_root.join("settings.xml").as_path(),
        "project-local",
        "项目 Maven 设置: settings.xml",
    );
    discover_direct_file_option(
        out,
        seen,
        "mvn_settings",
        project_root.join("maven/settings.xml").as_path(),
        "project-local",
        "项目 Maven 设置: maven/settings.xml",
    );

    for candidate in [".gradle", "gradle-home", ".gradle-user-home"] {
        let path = project_root.join(candidate);
        if path.is_dir() {
            push_option_with_label(
                out,
                seen,
                "gradle_user_home",
                normalize_path(path.as_path()),
                "project-local",
                None,
                Some(format!("项目 Gradle Home: {}", candidate)),
                false,
            );
        }
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use tokio::process::Command;

const DOCKER_BIN_ENV: &str = "LOCAL_CONNECTOR_DOCKER_BIN";

pub(crate) fn docker_command() -> Command {
    Command::new(docker_executable())
}

pub(crate) fn docker_executable() -> PathBuf {
    if let Some(configured) = std::env::var_os(DOCKER_BIN_ENV)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return configured;
    }

    let executable_name = if cfg!(windows) {
        "docker.exe"
    } else {
        "docker"
    };
    if let Some(path) = executable_in_path(executable_name) {
        return path;
    }

    for candidate in platform_candidates() {
        if candidate.is_file() {
            return candidate;
        }
    }

    PathBuf::from(executable_name)
}

fn executable_in_path(executable_name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(executable_name))
        .find(|candidate| candidate.is_file())
}

fn platform_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(Path::new(&home).join(".docker/bin/docker"));
        }
        candidates.extend([
            PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker"),
            PathBuf::from("/opt/homebrew/bin/docker"),
            PathBuf::from("/usr/local/bin/docker"),
        ]);
    } else if cfg!(windows) {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            candidates
                .push(Path::new(&program_files).join("Docker/Docker/resources/bin/docker.exe"));
        }
        if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
            candidates
                .push(Path::new(&program_files_x86).join("Docker/Docker/resources/bin/docker.exe"));
        }
    } else {
        candidates.extend([
            PathBuf::from("/usr/bin/docker"),
            PathBuf::from("/usr/local/bin/docker"),
            PathBuf::from("/snap/bin/docker"),
        ]);
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::docker_executable;

    #[test]
    fn resolves_a_real_docker_cli_when_docker_desktop_is_installed() {
        let docker_desktop =
            std::path::Path::new("/Applications/Docker.app/Contents/Resources/bin/docker");
        if cfg!(target_os = "macos") && docker_desktop.is_file() {
            let resolved = docker_executable();
            assert!(resolved.is_file(), "resolved={}", resolved.display());
        }
    }
}

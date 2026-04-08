use std::path::Path;

pub(super) fn select_shell() -> String {
    if cfg!(windows) {
        if let Ok(comspec) = std::env::var("COMSPEC") {
            let trimmed = comspec.trim();
            if !trimmed.is_empty() && Path::new(trimmed).exists() {
                return trimmed.to_string();
            }
        }
        if let Some(path) = find_in_path(&["cmd.exe", "cmd"]) {
            return path;
        }
        if let Some(path) = find_in_path(&["pwsh.exe", "pwsh"]) {
            return path;
        }
        if let Some(path) = find_in_path(&["powershell.exe", "powershell"]) {
            return path;
        }
        return "cmd.exe".to_string();
    }

    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.trim().is_empty() {
            return shell;
        }
    }
    if Path::new("/bin/bash").exists() {
        return "/bin/bash".to_string();
    }
    if Path::new("/bin/zsh").exists() {
        return "/bin/zsh".to_string();
    }
    "/bin/sh".to_string()
}

fn find_in_path(candidates: &[&str]) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in std::env::split_paths(&path_var) {
        for name in candidates {
            let full = dir.join(name);
            if full.exists() {
                return Some(full.to_string_lossy().to_string());
            }
        }
    }
    None
}

use super::types::ProjectContext;
use std::path::{Component, Path, PathBuf};

pub fn build_project_context(
    project_root: &str,
    file_path: &str,
) -> Result<ProjectContext, String> {
    let root_input = PathBuf::from(project_root.trim());
    if project_root.trim().is_empty() {
        return Err("project_root 不能为空".to_string());
    }
    if !root_input.exists() {
        return Err("project_root 不存在".to_string());
    }
    if !root_input.is_dir() {
        return Err("project_root 不是目录".to_string());
    }

    let file_input = PathBuf::from(file_path.trim());
    if file_path.trim().is_empty() {
        return Err("file_path 不能为空".to_string());
    }
    if !file_input.exists() {
        return Err("file_path 不存在".to_string());
    }
    if !file_input.is_file() {
        return Err("file_path 不是文件".to_string());
    }

    let normalized_root = normalize_path(&root_input);
    let normalized_file = ensure_path_inside_root(&normalized_root, &file_input)?;
    let relative_path = pathdiff::diff_paths(&normalized_file, &normalized_root)
        .unwrap_or_else(|| normalized_file.clone())
        .to_string_lossy()
        .to_string();

    Ok(ProjectContext {
        root: normalized_root,
        file_path: normalized_file.clone(),
        relative_path,
        language: detect_language(&normalized_file),
    })
}

pub fn detect_language(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if file_name == "dockerfile" {
        return "dockerfile".to_string();
    }

    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "java" => "java",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "rs" => "rust",
        "go" => "go",
        "py" => "python",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "php" => "php",
        "rb" => "ruby",
        "cs" => "csharp",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "h" | "hxx" | "ipp" => "cpp",
        "c" => "c",
        "sh" | "bash" | "zsh" => "shell",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "md" => "markdown",
        _ => "unknown",
    }
    .to_string()
}

fn ensure_path_inside_root(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let candidate = if target.is_absolute() {
        target.to_path_buf()
    } else {
        root.join(target)
    };
    let normalized = normalize_path(&candidate);
    if !normalized.starts_with(root) {
        return Err("file_path 超出项目根目录".to_string());
    }
    Ok(normalized)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => components.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = components.pop() {
                    if matches!(last, Component::Prefix(_) | Component::RootDir) {
                        components.push(last);
                    }
                }
            }
            Component::Normal(_) => components.push(component),
        }
    }

    let mut normalized = PathBuf::new();
    for component in components {
        normalized.push(component.as_os_str());
    }
    normalized
}

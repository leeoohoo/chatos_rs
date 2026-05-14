use super::types::ProjectContext;
use crate::core::path_guard::{canonicalize_existing_dir, path_is_within_root};
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

    let normalized_root =
        canonicalize_existing_dir(&root_input).map_err(|_| "project_root 不是目录".to_string())?;
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
    let canonical =
        std::fs::canonicalize(&candidate).map_err(|_| "file_path 不存在".to_string())?;
    let normalized = normalize_path(canonical.as_path());
    if !path_is_within_root(&normalized, root) {
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

#[cfg(test)]
mod tests {
    use super::build_project_context;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("{}_{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    #[cfg(unix)]
    #[test]
    fn accepts_symlink_project_root_when_file_resolves_inside_real_root() {
        use std::os::unix::fs::symlink;

        let actual_root = make_temp_dir("code_nav_workspace_actual_root");
        let link_parent = make_temp_dir("code_nav_workspace_link_parent");
        let linked_root = link_parent.join("linked-root");
        symlink(&actual_root, &linked_root).expect("create symlink root");

        let file_path = actual_root.join("main.rs");
        fs::write(&file_path, "fn main() {}\n").expect("write file");

        let ctx = build_project_context(
            linked_root.to_string_lossy().as_ref(),
            file_path.to_string_lossy().as_ref(),
        )
        .expect("build project context");

        assert_eq!(
            ctx.root,
            fs::canonicalize(&actual_root).expect("canonical root")
        );
        assert_eq!(
            ctx.file_path,
            fs::canonicalize(&file_path).expect("canonical file")
        );
        assert_eq!(ctx.relative_path, "main.rs");

        fs::remove_dir_all(&actual_root).expect("cleanup actual root");
        fs::remove_dir_all(&link_parent).expect("cleanup link parent");
    }

    #[test]
    fn rejects_file_outside_root_even_when_absolute() {
        let root = make_temp_dir("code_nav_workspace_root");
        let outside = make_temp_dir("code_nav_workspace_outside");
        let inside_file = root.join("main.rs");
        let outside_file = outside.join("other.rs");
        fs::write(&inside_file, "fn main() {}\n").expect("write inside file");
        fs::write(&outside_file, "fn other() {}\n").expect("write outside file");

        let err = build_project_context(
            root.to_string_lossy().as_ref(),
            outside_file.to_string_lossy().as_ref(),
        )
        .expect_err("reject file outside root");
        assert_eq!(err, "file_path 超出项目根目录");

        fs::remove_dir_all(&root).expect("cleanup root");
        fs::remove_dir_all(&outside).expect("cleanup outside");
    }
}

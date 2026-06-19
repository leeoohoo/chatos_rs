use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use super::process::{DEFAULT_GIT_TIMEOUT, git_output};

pub(super) async fn require_repo_root(root: &str) -> Result<PathBuf, String> {
    let root = parse_root(root)?;
    discover_repo_root(root.as_path())
        .await?
        .ok_or_else(|| "当前项目不是 Git 仓库".to_string())
}

pub(super) fn parse_root(root: &str) -> Result<PathBuf, String> {
    let root = root.trim();
    if root.is_empty() {
        return Err("root 不能为空".to_string());
    }
    let path = PathBuf::from(root);
    if !path.exists() {
        return Err("root 路径不存在".to_string());
    }
    if !path.is_dir() {
        return Err("root 不是目录".to_string());
    }
    std::fs::canonicalize(path).map_err(|err| format!("解析 root 路径失败: {}", err))
}

pub async fn discover_repo_root(root: &Path) -> Result<Option<PathBuf>, String> {
    match git_output(root, ["rev-parse", "--show-toplevel"], DEFAULT_GIT_TIMEOUT).await {
        Ok(output) => {
            let text = output.stdout.trim();
            if text.is_empty() {
                Ok(None)
            } else {
                let repo_root = std::fs::canonicalize(text).unwrap_or_else(|_| PathBuf::from(text));
                if !root.starts_with(repo_root.as_path()) {
                    return Err("Git 仓库根目录不在当前项目路径内".to_string());
                }
                Ok(Some(repo_root))
            }
        }
        Err(message)
            if message.contains("not a git repository") || message.contains("不是 git 仓库") =>
        {
            Ok(None)
        }
        Err(message) => Err(message),
    }
}

pub async fn discover_child_repo_roots(root: &Path, limit: usize) -> Result<Vec<PathBuf>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut discovered = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if discovered.len() >= limit {
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => continue,
            };
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(
                name.as_ref(),
                ".git"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | ".next"
                    | ".nuxt"
                    | ".turbo"
                    | ".cache"
            ) {
                continue;
            }
            match discover_repo_root(path.as_path()).await? {
                Some(repo_root) if repo_root == path => {
                    discovered.insert(repo_root);
                    if discovered.len() >= limit {
                        break;
                    }
                }
                _ => {
                    stack.push(path);
                }
            }
        }
    }
    Ok(discovered.into_iter().collect())
}

pub(super) fn parse_optional_root(root: Option<&str>) -> Result<Option<PathBuf>, String> {
    match root {
        Some(value) if !value.trim().is_empty() => parse_root(value).map(Some),
        _ => Ok(None),
    }
}

pub(super) async fn validate_branch_name(repo_root: &Path, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("分支名不能为空".to_string());
    }
    if name.starts_with('-') || name.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err("分支名不合法".to_string());
    }
    git_output(
        repo_root,
        vec!["check-ref-format", "--branch", name],
        DEFAULT_GIT_TIMEOUT,
    )
    .await
    .map(|_| ())
    .map_err(|_| "分支名不合法".to_string())
}

pub(super) fn merge_args<'a>(mode: Option<&str>, branch: &'a str) -> Result<Vec<&'a str>, String> {
    match mode.unwrap_or("default").trim() {
        "" | "default" => Ok(vec!["merge", "--no-edit", branch]),
        "no-ff" => Ok(vec!["merge", "--no-ff", "--no-edit", branch]),
        "ff-only" => Ok(vec!["merge", "--ff-only", branch]),
        _ => Err("不支持的 merge 模式".to_string()),
    }
}

pub(super) fn validate_relative_paths(paths: &[String]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err("paths 不能为空".to_string());
    }
    let mut out = Vec::new();
    for raw in paths {
        let path = raw.trim().replace('\\', "/");
        if path.is_empty() {
            continue;
        }
        let parsed = Path::new(&path);
        if parsed.is_absolute()
            || parsed.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err("paths 只能是仓库内相对路径".to_string());
        }
        out.push(path);
    }
    if out.is_empty() {
        return Err("paths 不能为空".to_string());
    }
    Ok(out)
}

pub(super) fn ensure_safe_ref(value: &str, label: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') || value.chars().any(|ch| ch.is_control()) {
        return Err(format!("{} 不合法", label));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{merge_args, validate_relative_paths};

    #[test]
    fn builds_merge_args_without_editor() {
        assert_eq!(
            merge_args(None, "feature").expect("default merge args"),
            vec!["merge", "--no-edit", "feature"]
        );
        assert_eq!(
            merge_args(Some("no-ff"), "feature").expect("no-ff merge args"),
            vec!["merge", "--no-ff", "--no-edit", "feature"]
        );
        assert_eq!(
            merge_args(Some("ff-only"), "feature").expect("ff-only merge args"),
            vec!["merge", "--ff-only", "feature"]
        );
        assert!(merge_args(Some("squash"), "feature").is_err());
    }

    #[test]
    fn rejects_non_relative_paths() {
        assert!(validate_relative_paths(&["../secret".to_string()]).is_err());
        assert!(validate_relative_paths(&["/etc/passwd".to_string()]).is_err());
        assert!(validate_relative_paths(&["safe/file.rs".to_string()]).is_ok());
    }
}

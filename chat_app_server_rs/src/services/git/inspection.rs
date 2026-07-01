// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use tokio::fs;

use super::process::{git_output, DEFAULT_GIT_TIMEOUT};

const MAX_UNTRACKED_DIFF_BYTES: u64 = 256 * 1024;

pub(super) async fn ahead_behind(
    repo_root: &Path,
    branch: &str,
    upstream: &str,
) -> Result<(usize, usize), String> {
    let range = format!("{}...{}", branch, upstream);
    let output = git_output(
        repo_root,
        vec!["rev-list", "--left-right", "--count", range.as_str()],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    let mut parts = output.stdout.split_whitespace();
    let ahead = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let behind = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    Ok((ahead, behind))
}

pub(super) async fn is_tracked_path(repo_root: &Path, path: &str) -> bool {
    git_output(
        repo_root,
        vec!["ls-files", "--error-unmatch", "--", path],
        DEFAULT_GIT_TIMEOUT,
    )
    .await
    .is_ok()
}

pub(super) async fn untracked_file_patch(repo_root: &Path, path: &str) -> Result<String, String> {
    let absolute_path = repo_root.join(path);
    let metadata = fs::symlink_metadata(absolute_path.as_path())
        .await
        .map_err(|err| format!("读取未跟踪文件失败: {}", err))?;
    if metadata.file_type().is_symlink() {
        return Err("未跟踪符号链接暂不支持预览 diff".to_string());
    }
    if !metadata.is_file() {
        return Err("只能预览文件 diff".to_string());
    }
    let canonical_path = std::fs::canonicalize(absolute_path.as_path())
        .map_err(|err| format!("解析未跟踪文件路径失败: {}", err))?;
    let canonical_repo_root = std::fs::canonicalize(repo_root)
        .map_err(|err| format!("解析 Git 仓库路径失败: {}", err))?;
    if !canonical_path.starts_with(canonical_repo_root.as_path()) {
        return Err("未跟踪文件不在 Git 仓库内，已拒绝预览".to_string());
    }
    if metadata.len() > MAX_UNTRACKED_DIFF_BYTES {
        return Ok(format!(
            "diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n@@ -0,0 +1 @@\n+未跟踪文件过大，已跳过内容预览（{1} bytes）。\n",
            path,
            metadata.len()
        ));
    }
    let bytes = fs::read(canonical_path.as_path())
        .await
        .map_err(|err| format!("读取未跟踪文件失败: {}", err))?;
    let content = match String::from_utf8(bytes) {
        Ok(value) => value,
        Err(_) => {
            return Ok(format!(
                "diff --git a/{0} b/{0}\nnew file mode 100644\nBinary file b/{0} differs\n",
                path
            ));
        }
    };
    let line_count = content.lines().count().max(1);
    let mut patch = format!(
        "diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n@@ -0,0 +1,{1} @@\n",
        path, line_count
    );
    if content.is_empty() {
        return Ok(patch);
    }
    for line in content.lines() {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    if !content.ends_with('\n') {
        patch.push_str("\\ No newline at end of file\n");
    }
    Ok(patch)
}

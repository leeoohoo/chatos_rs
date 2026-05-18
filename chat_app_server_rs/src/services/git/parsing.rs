use std::path::{Path, PathBuf};

use super::contracts::{GitChangeCounts, GitCompareCommit, GitDiffFile, GitStatusFile, GitSummary};

pub(super) fn summary_from_status(repo_root: PathBuf, status: &str) -> GitSummary {
    let mut head = None;
    let mut current_branch = None;
    let mut detached = false;
    let mut upstream = None;
    let mut ahead = 0usize;
    let mut behind = 0usize;
    let mut changes = GitChangeCounts {
        staged: 0,
        unstaged: 0,
        untracked: 0,
        conflicted: 0,
    };

    for line in status.lines() {
        if let Some(value) = line.strip_prefix("# branch.oid ") {
            head = non_empty(value);
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.head ") {
            let value = value.trim();
            if value == "(detached)" {
                detached = true;
            } else {
                current_branch = non_empty(value);
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.upstream ") {
            upstream = non_empty(value);
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.ab ") {
            for part in value.split_whitespace() {
                if let Some(raw) = part.strip_prefix('+') {
                    ahead = raw.parse().unwrap_or(0);
                } else if let Some(raw) = part.strip_prefix('-') {
                    behind = raw.parse().unwrap_or(0);
                }
            }
            continue;
        }
        count_status_line(line, &mut changes);
    }

    let dirty = changes.staged > 0
        || changes.unstaged > 0
        || changes.untracked > 0
        || changes.conflicted > 0;
    GitSummary {
        is_repo: true,
        root: Some(repo_root.to_string_lossy().to_string()),
        worktree_root: Some(repo_root.to_string_lossy().to_string()),
        query_root: None,
        resolved_root: None,
        selected_root: None,
        head,
        current_branch,
        detached,
        upstream,
        ahead,
        behind,
        dirty,
        operation_state: detect_operation_state(repo_root.as_path()),
        changes,
        available_repositories: Vec::new(),
    }
}

pub(super) fn non_repo_summary() -> GitSummary {
    GitSummary {
        is_repo: false,
        root: None,
        worktree_root: None,
        query_root: None,
        resolved_root: None,
        selected_root: None,
        head: None,
        current_branch: None,
        detached: false,
        upstream: None,
        ahead: 0,
        behind: 0,
        dirty: false,
        operation_state: None,
        changes: GitChangeCounts {
            staged: 0,
            unstaged: 0,
            untracked: 0,
            conflicted: 0,
        },
        available_repositories: Vec::new(),
    }
}

pub(super) fn parse_status_files(raw: &str) -> Vec<GitStatusFile> {
    let mut files = Vec::new();
    let mut parts = raw.split('\0').peekable();
    while let Some(record) = parts.next() {
        if record.is_empty() || record.starts_with('#') {
            continue;
        }
        if let Some(path) = record.strip_prefix("? ") {
            files.push(GitStatusFile {
                path: path.to_string(),
                old_path: None,
                status: "untracked".to_string(),
                staged: false,
                unstaged: false,
                conflicted: false,
            });
            continue;
        }
        if record.starts_with("u ") {
            if let Some((xy, path)) = parse_status_record(record, 10) {
                files.push(GitStatusFile {
                    path,
                    old_path: None,
                    status: status_from_xy(xy, true),
                    staged: true,
                    unstaged: true,
                    conflicted: true,
                });
            }
            continue;
        }
        if record.starts_with("2 ") {
            if let Some((xy, path)) = parse_status_record(record, 9) {
                let old_path = parts.next().map(ToOwned::to_owned);
                files.push(GitStatusFile {
                    path,
                    old_path,
                    status: status_from_xy(xy, false),
                    staged: xy.chars().next().unwrap_or('.') != '.',
                    unstaged: xy.chars().nth(1).unwrap_or('.') != '.',
                    conflicted: false,
                });
            }
            continue;
        }
        if record.starts_with("1 ") {
            if let Some((xy, path)) = parse_status_record(record, 8) {
                files.push(GitStatusFile {
                    path,
                    old_path: None,
                    status: status_from_xy(xy, false),
                    staged: xy.chars().next().unwrap_or('.') != '.',
                    unstaged: xy.chars().nth(1).unwrap_or('.') != '.',
                    conflicted: false,
                });
            }
        }
    }
    files
}

pub(super) fn parse_name_status_z(raw: &str) -> Vec<GitDiffFile> {
    let mut files = Vec::new();
    let mut parts = raw.split('\0').filter(|part| !part.is_empty()).peekable();
    while let Some(status) = parts.next() {
        let Some(path) = parts.next() else {
            break;
        };
        let code = status.chars().next().unwrap_or('M');
        if matches!(code, 'R' | 'C') {
            let Some(new_path) = parts.next() else {
                break;
            };
            files.push(GitDiffFile {
                path: new_path.to_string(),
                old_path: Some(path.to_string()),
                status: status_from_name_status(code),
            });
        } else {
            files.push(GitDiffFile {
                path: path.to_string(),
                old_path: None,
                status: status_from_name_status(code),
            });
        }
    }
    files
}

pub(super) fn parse_compare_commits(raw: &str) -> Vec<GitCompareCommit> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\x1f');
            let side = match parts.next()?.trim() {
                "<" => "current",
                ">" => "target",
                _ => "unknown",
            };
            let hash = parts.next()?.trim();
            let subject = parts.next()?.trim();
            if hash.is_empty() {
                return None;
            }
            Some(GitCompareCommit {
                side: side.to_string(),
                hash: hash.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect()
}

pub(super) fn split_remote_branch(name: &str) -> (Option<String>, Option<String>) {
    let mut parts = name.splitn(2, '/');
    let remote = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let short_name = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    (remote, short_name)
}

pub(super) fn compact_output(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(1200).collect())
    }
}

pub(super) fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn count_status_line(line: &str, changes: &mut GitChangeCounts) {
    if let Some(rest) = line.strip_prefix("1 ").or_else(|| line.strip_prefix("2 ")) {
        count_xy(rest, changes);
    } else if line.starts_with("u ") {
        changes.conflicted += 1;
    } else if line.starts_with("? ") {
        changes.untracked += 1;
    }
}

fn count_xy(rest: &str, changes: &mut GitChangeCounts) {
    let xy = rest.split_whitespace().next().unwrap_or("");
    let mut chars = xy.chars();
    let staged = chars.next().unwrap_or('.');
    let unstaged = chars.next().unwrap_or('.');
    if staged != '.' {
        changes.staged += 1;
    }
    if unstaged != '.' {
        changes.unstaged += 1;
    }
}

fn parse_status_record(record: &str, space_count_before_path: usize) -> Option<(&str, String)> {
    let xy = record.split_whitespace().nth(1)?;
    let mut seen = 0usize;
    for (index, ch) in record.char_indices() {
        if ch == ' ' {
            seen += 1;
            if seen == space_count_before_path {
                return Some((xy, record[index + 1..].to_string()));
            }
        }
    }
    None
}

fn status_from_name_status(code: char) -> String {
    match code {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'U' => "conflicted",
        _ => "modified",
    }
    .to_string()
}

fn status_from_xy(xy: &str, conflicted: bool) -> String {
    if conflicted {
        return "conflicted".to_string();
    }
    if xy.contains('R') {
        return "renamed".to_string();
    }
    if xy.contains('C') {
        return "copied".to_string();
    }
    if xy.contains('D') {
        return "deleted".to_string();
    }
    if xy.contains('A') {
        return "added".to_string();
    }
    "modified".to_string()
}

fn detect_operation_state(repo_root: &Path) -> Option<String> {
    let git_dir = repo_root.join(".git");
    if git_dir.join("MERGE_HEAD").exists() {
        return Some("merge".to_string());
    }
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        return Some("rebase".to_string());
    }
    if git_dir.join("CHERRY_PICK_HEAD").exists() {
        return Some("cherry-pick".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        parse_compare_commits, parse_name_status_z, parse_status_files, summary_from_status,
    };

    #[test]
    fn parses_summary_from_porcelain_v2_branch_status() {
        let status = "\
# branch.oid abc123
# branch.head main
# branch.upstream origin/main
# branch.ab +2 -1
1 .M N... 100644 100644 100644 abc abc src/main.rs
1 A. N... 000000 100644 100644 000 abc src/new.rs
? src/loose.rs
";
        let summary = summary_from_status(PathBuf::from("/tmp/repo"), status);
        assert!(summary.is_repo);
        assert_eq!(summary.current_branch.as_deref(), Some("main"));
        assert_eq!(summary.ahead, 2);
        assert_eq!(summary.behind, 1);
        assert_eq!(summary.changes.unstaged, 1);
        assert_eq!(summary.changes.staged, 1);
        assert_eq!(summary.changes.untracked, 1);
        assert!(summary.dirty);
    }

    #[test]
    fn parses_porcelain_v2_z_status_files() {
        let raw = "1 .M N... 100644 100644 100644 abc abc src/main.rs\0\
2 R. N... 100644 100644 100644 abc def R100 src/new name.rs\0src/old name.rs\0\
? src/loose file.rs\0";
        let files = parse_status_files(raw);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, "modified");
        assert!(!files[0].staged);
        assert!(files[0].unstaged);
        assert_eq!(files[1].path, "src/new name.rs");
        assert_eq!(files[1].old_path.as_deref(), Some("src/old name.rs"));
        assert_eq!(files[1].status, "renamed");
        assert!(files[1].staged);
        assert!(!files[1].unstaged);
        assert_eq!(files[2].status, "untracked");
    }

    #[test]
    fn parses_name_status_z_diff_files() {
        let raw = "M\0src/main.rs\0R100\0src/old.rs\0src/new.rs\0D\0src/deleted.rs\0";
        let files = parse_name_status_z(raw);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, "modified");
        assert_eq!(files[1].status, "renamed");
        assert_eq!(files[1].old_path.as_deref(), Some("src/old.rs"));
        assert_eq!(files[1].path, "src/new.rs");
        assert_eq!(files[2].status, "deleted");
    }

    #[test]
    fn parses_compare_commits() {
        let commits = parse_compare_commits(
            "<\u{1f}abc123\u{1f}current only\n>\u{1f}def456\u{1f}target only\n",
        );
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].side, "current");
        assert_eq!(commits[0].hash, "abc123");
        assert_eq!(commits[1].side, "target");
    }
}

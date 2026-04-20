use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct GitRootQuery {
    pub root: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitFetchRequest {
    pub root: String,
    pub remote: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitPullRequest {
    pub root: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitPushRequest {
    pub root: String,
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub set_upstream: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitCheckoutRequest {
    pub root: String,
    pub branch: Option<String>,
    pub remote_branch: Option<String>,
    pub create_tracking: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitCreateBranchRequest {
    pub root: String,
    pub name: String,
    pub start_point: Option<String>,
    pub checkout: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitMergeRequest {
    pub root: String,
    pub branch: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitPathRequest {
    pub root: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitCommitRequest {
    pub root: String,
    pub message: String,
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitCompareQuery {
    pub root: String,
    pub target: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitDiffQuery {
    pub root: String,
    pub path: String,
    pub target: Option<String>,
    pub staged: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitChangeCounts {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitClientInfo {
    pub available: bool,
    pub source: String,
    pub path: String,
    pub version: Option<String>,
    pub error: Option<String>,
    pub bundled_candidates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitSummary {
    pub is_repo: bool,
    pub root: Option<String>,
    pub worktree_root: Option<String>,
    pub head: Option<String>,
    pub current_branch: Option<String>,
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub dirty: bool,
    pub operation_state: Option<String>,
    pub changes: GitChangeCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitBranchInfo {
    pub name: String,
    pub short_name: Option<String>,
    pub current: bool,
    pub upstream: Option<String>,
    pub remote: Option<String>,
    pub tracked_by: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub last_commit: Option<String>,
    pub last_commit_subject: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitBranches {
    pub current: Option<String>,
    pub locals: Vec<GitBranchInfo>,
    pub remotes: Vec<GitBranchInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitStatusFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
    pub staged: bool,
    pub unstaged: bool,
    pub conflicted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitStatus {
    pub files: Vec<GitStatusFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitDiffFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitCompareCommit {
    pub side: String,
    pub hash: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitCompareResult {
    pub current: String,
    pub target: String,
    pub files: Vec<GitDiffFile>,
    pub commits: Vec<GitCompareCommit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitFileDiff {
    pub path: String,
    pub target: Option<String>,
    pub staged: bool,
    pub patch: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitActionResult {
    pub success: bool,
    pub summary: GitSummary,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

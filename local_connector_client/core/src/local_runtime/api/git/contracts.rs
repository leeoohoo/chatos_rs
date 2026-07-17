// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct GitRootQuery {
    pub(super) root: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitCompareQuery {
    pub(super) root: String,
    pub(super) target: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitDiffQuery {
    pub(super) root: String,
    pub(super) path: String,
    pub(super) target: Option<String>,
    pub(super) staged: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitFetchRequest {
    pub(super) root: String,
    pub(super) remote: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitPullRequest {
    pub(super) root: String,
    pub(super) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitPushRequest {
    pub(super) root: String,
    pub(super) remote: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) set_upstream: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitCheckoutRequest {
    pub(super) root: String,
    pub(super) branch: Option<String>,
    pub(super) remote_branch: Option<String>,
    pub(super) create_tracking: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitCreateBranchRequest {
    pub(super) root: String,
    pub(super) name: String,
    pub(super) start_point: Option<String>,
    pub(super) checkout: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitMergeRequest {
    pub(super) root: String,
    pub(super) branch: String,
    pub(super) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitPathRequest {
    pub(super) root: String,
    pub(super) paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitCommitRequest {
    pub(super) root: String,
    pub(super) message: String,
    pub(super) paths: Option<Vec<String>>,
}

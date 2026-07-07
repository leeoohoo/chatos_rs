// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use crate::relay::RelayRequest;
use crate::{LocalState, WorkspaceState};

mod normalize;
mod resolve;

pub(crate) use normalize::{
    normalize_relative_workspace_path, normalize_request_workspace_relative_path,
};
pub(crate) use resolve::{
    canonicalize_existing_dir, relative_to_workspace, resolve_request_workspace_dir,
    resolve_workspace_dir, workspace_fingerprint,
};

#[cfg(test)]
pub(crate) use resolve::resolve_request_workspace_path;

pub(crate) fn workspace_for_request<'a>(
    state: &'a LocalState,
    workspace_id: &str,
) -> Result<&'a WorkspaceState> {
    state
        .workspace_by_id(workspace_id)
        .ok_or_else(|| anyhow!("workspace is not registered locally: {workspace_id}"))
}

pub(crate) fn request_cwd(request: &RelayRequest) -> Option<&str> {
    request
        .headers
        .get("x-local-connector-cwd")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
}

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
    resolve_workspace_dir, resolve_workspace_path, workspace_fingerprint,
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

pub(crate) fn request_default_tool_root(request: &RelayRequest) -> Result<Option<String>> {
    request
        .headers
        .get("x-local-connector-default-tool-root")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
        .map(normalize_relative_workspace_path)
        .transpose()
}

pub(crate) fn request_owned_paths(request: &RelayRequest) -> Result<Option<Vec<String>>> {
    let Some(value) = request
        .headers
        .get("x-local-connector-owned-paths")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let decoded = urlencoding::decode(value)
        .map_err(|error| anyhow!("invalid Local Connector owned paths encoding: {error}"))?;
    let values = serde_json::from_str::<Vec<String>>(decoded.as_ref())
        .map_err(|error| anyhow!("invalid Local Connector owned paths header: {error}"))?;
    let mut normalized = values
        .into_iter()
        .map(|path| normalize_relative_workspace_path(path.as_str()))
        .collect::<Result<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    if normalized.is_empty() {
        Ok(None)
    } else {
        Ok(Some(normalized))
    }
}

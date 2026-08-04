// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalConnectorWorkspaceRef {
    pub device_id: String,
    pub workspace_id: String,
    pub relative_path: Option<String>,
}

pub fn normalize_local_connector_relative_path(value: Option<&str>) -> Option<String> {
    let value = value?.trim().replace('\\', "/");
    let value = value.trim_matches('/');
    if value.is_empty() || value == "." {
        return None;
    }
    let parts = value
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

pub fn local_connector_relative_path_is_safe(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path.split('/').all(|part| {
            let part = part.trim();
            !part.is_empty() && part != "." && part != ".."
        })
}

pub fn parse_local_connector_workspace_root(root_path: &str) -> Option<LocalConnectorWorkspaceRef> {
    let rest = root_path.trim().strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX)?;
    let mut parts = rest.splitn(3, '/');
    let device_id = normalized_text(parts.next()?)?.to_string();
    let workspace_id = normalized_text(parts.next()?)?.to_string();
    let relative_path = match parts.next() {
        Some(path) => Some(decode_relative_path(path)?),
        None => None,
    };
    Some(LocalConnectorWorkspaceRef {
        device_id,
        workspace_id,
        relative_path,
    })
}

pub fn local_connector_workspace_root(
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> Option<String> {
    let device_id = normalized_text(device_id)?;
    let workspace_id = normalized_text(workspace_id)?;
    let base = format!("{LOCAL_CONNECTOR_ROOT_PREFIX}{device_id}/{workspace_id}");
    match relative_path.and_then(|value| normalize_local_connector_relative_path(Some(value))) {
        Some(relative_path) if local_connector_relative_path_is_safe(relative_path.as_str()) => {
            Some(format!(
                "{base}/{}",
                encode_relative_path(relative_path.as_str())
            ))
        }
        Some(_) => None,
        None => Some(base),
    }
}

fn encode_relative_path(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.trim().is_empty())
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_relative_path(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/').filter(|part| !part.trim().is_empty()) {
        parts.push(urlencoding::decode(part).ok()?.into_owned());
    }
    let joined = parts.join("/");
    normalize_local_connector_relative_path(Some(joined.as_str()))
        .filter(|path| local_connector_relative_path_is_safe(path))
}

fn normalized_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_workspace_paths_round_trip_without_exposing_absolute_paths() {
        let root =
            local_connector_workspace_root("device-1", "workspace-1", Some("apps/my backend"))
                .expect("connector root");
        assert_eq!(
            root,
            "local://connector/device-1/workspace-1/apps/my%20backend"
        );
        assert_eq!(
            parse_local_connector_workspace_root(root.as_str()),
            Some(LocalConnectorWorkspaceRef {
                device_id: "device-1".to_string(),
                workspace_id: "workspace-1".to_string(),
                relative_path: Some("apps/my backend".to_string()),
            })
        );
    }

    #[test]
    fn connector_workspace_paths_reject_parent_traversal() {
        assert!(
            local_connector_workspace_root("device-1", "workspace-1", Some("apps/../secrets"))
                .is_none()
        );
        assert!(parse_local_connector_workspace_root(
            "local://connector/device-1/workspace-1/apps/%2E%2E/secrets"
        )
        .is_none());
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use tracing::{info, warn};

use crate::api::projects::memory_sync::sync_active_project;
use crate::models::project::{Project, ProjectService};

use super::connector_client::{connector_get_json, connector_put_json};
use super::root_path::{local_connector_root_path, parse_local_connector_root_path};
use super::types::{
    LocalConnectorDevice, LocalConnectorProjectBinding, LocalConnectorWorkspace,
    UpdateProjectBindingRequest,
};
use super::{LOCAL_CONNECTOR_DEVICE_ONLINE, LOCAL_CONNECTOR_WORKSPACE_ACTIVE};

pub(crate) async fn reconcile_local_connector_project(mut project: Project) -> Project {
    let Some(root_ref) = parse_local_connector_root_path(project.root_path.as_str()) else {
        return project;
    };
    let devices =
        match connector_get_json::<Vec<LocalConnectorDevice>>("/api/local-connectors/devices", &[])
            .await
        {
            Ok(devices) => devices,
            Err(_) => return project,
        };
    let workspaces = match connector_get_json::<Vec<LocalConnectorWorkspace>>(
        "/api/local-connectors/workspaces",
        &[],
    )
    .await
    {
        Ok(workspaces) => workspaces,
        Err(_) => return project,
    };

    let Some(replacement) = find_replacement_workspace(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        devices.as_slice(),
        workspaces.as_slice(),
    ) else {
        return project;
    };
    if replacement.device_id == root_ref.device_id && replacement.id == root_ref.workspace_id {
        return project;
    }

    let new_root_path = local_connector_root_path(
        replacement.device_id.as_str(),
        replacement.id.as_str(),
        root_ref.relative_path.as_deref(),
    );
    if let Err(err) = ProjectService::update(
        project.id.as_str(),
        None,
        Some(new_root_path.clone()),
        None,
        None,
    )
    .await
    {
        warn!(
            project_id = project.id.as_str(),
            old_device_id = root_ref.device_id.as_str(),
            new_device_id = replacement.device_id.as_str(),
            error = err.as_str(),
            "failed to migrate local connector project root"
        );
        return project;
    }

    project.root_path = new_root_path;
    reconcile_project_bindings(&project, replacement).await;
    if let Err(err) = sync_active_project(&project).await {
        warn!(
            project_id = project.id.as_str(),
            error = err.as_str(),
            "failed to sync migrated local connector project"
        );
    }
    info!(
        project_id = project.id.as_str(),
        device_id = replacement.device_id.as_str(),
        workspace_id = replacement.id.as_str(),
        "migrated local connector project to active device"
    );
    project
}

fn find_replacement_workspace<'a>(
    device_id: &str,
    workspace_id: &str,
    devices: &[LocalConnectorDevice],
    workspaces: &'a [LocalConnectorWorkspace],
) -> Option<&'a LocalConnectorWorkspace> {
    let is_online = |candidate_device_id: &str| {
        devices.iter().any(|device| {
            device.id == candidate_device_id && device.status == LOCAL_CONNECTOR_DEVICE_ONLINE
        })
    };

    if let Some(workspace) = workspaces.iter().find(|workspace| {
        workspace.id == workspace_id
            && workspace.status == LOCAL_CONNECTOR_WORKSPACE_ACTIVE
            && is_online(workspace.device_id.as_str())
    }) {
        return Some(workspace);
    }

    let original = workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id && workspace.device_id == device_id)?;
    workspaces
        .iter()
        .filter(|workspace| {
            workspace.status == LOCAL_CONNECTOR_WORKSPACE_ACTIVE
                && is_online(workspace.device_id.as_str())
        })
        .filter(|workspace| {
            !original.local_path_fingerprint.trim().is_empty()
                && workspace.local_path_fingerprint == original.local_path_fingerprint
        })
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
}

async fn reconcile_project_bindings(project: &Project, workspace: &LocalConnectorWorkspace) {
    let bindings = match connector_get_json::<Vec<LocalConnectorProjectBinding>>(
        "/api/local-connectors/project-bindings",
        &[("project_id", project.id.clone())],
    )
    .await
    {
        Ok(bindings) => bindings,
        Err(_) => return,
    };
    for binding in bindings {
        if binding.device_id == workspace.device_id && binding.workspace_id == workspace.id {
            continue;
        }
        let path = format!(
            "/api/local-connectors/project-bindings/{}",
            urlencoding::encode(binding.id.as_str())
        );
        if let Err((status, detail)) = connector_put_json::<Value, _>(
            path.as_str(),
            &UpdateProjectBindingRequest {
                device_id: workspace.device_id.as_str(),
                workspace_id: workspace.id.as_str(),
                enabled: binding.enabled,
            },
        )
        .await
        {
            warn!(
                project_id = project.id.as_str(),
                binding_id = binding.id.as_str(),
                %status,
                detail = ?detail.0,
                "failed to migrate local connector project binding"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::find_replacement_workspace;
    use crate::api::local_connectors::types::{LocalConnectorDevice, LocalConnectorWorkspace};

    fn device(id: &str, status: &str) -> LocalConnectorDevice {
        LocalConnectorDevice {
            id: id.to_string(),
            owner_user_id: "user-1".to_string(),
            display_name: id.to_string(),
            public_key: String::new(),
            client_version: None,
            os: None,
            status: status.to_string(),
            last_seen_at: None,
            revoked_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn workspace(
        id: &str,
        device_id: &str,
        fingerprint: &str,
        updated_at: &str,
    ) -> LocalConnectorWorkspace {
        LocalConnectorWorkspace {
            id: id.to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: device_id.to_string(),
            display_name: "workspace".to_string(),
            local_path_alias: "workspace".to_string(),
            local_path_fingerprint: fingerprint.to_string(),
            capabilities: vec![],
            status: "active".to_string(),
            created_at: String::new(),
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn replaces_offline_workspace_with_online_matching_fingerprint() {
        let devices = vec![device("old", "offline"), device("new", "online")];
        let workspaces = vec![
            workspace("old-workspace", "old", "same-root", "1"),
            workspace("new-workspace", "new", "same-root", "2"),
        ];
        let replacement = find_replacement_workspace("old", "old-workspace", &devices, &workspaces)
            .expect("replacement");
        assert_eq!(replacement.device_id, "new");
        assert_eq!(replacement.id, "new-workspace");
    }

    #[test]
    fn keeps_workspace_id_after_device_reregistration() {
        let devices = vec![device("old", "offline"), device("new", "online")];
        let workspaces = vec![workspace("workspace", "new", "same-root", "2")];
        let replacement = find_replacement_workspace("old", "workspace", &devices, &workspaces)
            .expect("replacement");
        assert_eq!(replacement.device_id, "new");
        assert_eq!(replacement.id, "workspace");
    }
}

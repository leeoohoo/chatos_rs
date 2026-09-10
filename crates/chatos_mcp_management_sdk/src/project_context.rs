// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ProjectExecutionContext, WorkspaceExecutionTarget, WorkspaceProviderKind};

/// A client declaration, NOT an authorization. There is intentionally no owner or absolute path.
/// The authenticated ingress must check the device/workspace grant before using this in a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProjectContextSnapshot {
    pub schema_version: u32,
    pub project_id: String,
    pub project_name: String,
    pub project_revision: i64,
    pub execution_target: ClientProjectExecutionTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProjectExecutionTarget {
    pub device_id: String,
    pub workspace_id: String,
    pub relative_root: String,
}

/// A control-plane authorization result, obtained over authenticated service transport.
/// This is persisted with a task, not used as a bearer token. Grants must be rechecked at use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectContextAuthorization {
    pub snapshot: ClientProjectContextSnapshot,
    pub owner_user_id: String,
    pub workspace_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizeProjectContextRequest {
    pub owner_user_id: String,
    pub snapshot: ClientProjectContextSnapshot,
}

impl ProjectContextAuthorization {
    pub fn validate(&self) -> Result<(), String> {
        self.snapshot.validate()?;
        validate_text(&self.owner_user_id, "authenticated owner", 256)?;
        validate_text(&self.workspace_fingerprint, "workspace fingerprint", 512)
    }

    pub fn validate_expected(
        &self,
        owner: &str,
        snapshot: &ClientProjectContextSnapshot,
    ) -> Result<(), String> {
        self.validate()?;
        if self.owner_user_id != owner || &self.snapshot != snapshot {
            return Err(
                "project authorization response does not match the requested owner/snapshot"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn execution_context(&self) -> Result<ProjectExecutionContext, String> {
        self.validate()?;
        // Include the registered binding, so moving a workspace with the same ID cannot silently
        // reuse a previously prepared runtime session or Plugin selection revision.
        let identity = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        let revision = format!(
            "client-project-v1:{}",
            hex::encode(Sha256::digest(identity))
        );
        Ok(ProjectExecutionContext {
            project_id: Some(self.snapshot.project_id.clone()),
            project_name: Some(self.snapshot.project_name.clone()),
            owner_user_id: self.owner_user_id.clone(),
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some(self.snapshot.execution_target.device_id.clone()),
                workspace_id: self.snapshot.execution_target.workspace_id.clone(),
                relative_root: Some(self.snapshot.execution_target.relative_root.clone()),
            }),
            revision,
        })
    }
}

impl ClientProjectContextSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported client project snapshot schemaVersion".to_string());
        }
        validate_text(&self.project_id, "projectId", 256)?;
        validate_text(&self.project_name, "projectName", 1024)?;
        if self.project_revision <= 0 || self.project_revision == i64::MAX {
            return Err("invalid client project snapshot projectRevision".to_string());
        }
        validate_route_id(&self.execution_target.device_id, "deviceId")?;
        validate_route_id(&self.execution_target.workspace_id, "workspaceId")?;
        let root = &self.execution_target.relative_root;
        if root.len() > 4096
            || root.trim() != root
            || root.chars().any(char::is_control)
            || root.contains(['\\', ':'])
            || (!root.is_empty() && root.split('/').any(|part| matches!(part, "" | "." | "..")))
        {
            return Err("invalid client project snapshot relativeRoot".to_string());
        }
        Ok(())
    }

    pub fn validate_project_id(&self, project_id: &str) -> Result<(), String> {
        self.validate()?;
        if self.project_id != project_id {
            return Err("client project snapshot project identity does not match".to_string());
        }
        Ok(())
    }

    /// Syntax validation only; does not grant access to any device or workspace.
    pub fn validate_for_owner(&self, owner_user_id: &str) -> Result<(), String> {
        self.validate()?;
        validate_text(owner_user_id, "authenticated owner", 256)?;
        Ok(())
    }
}

fn validate_text(value: &str, field: &str, max_bytes: usize) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        return Err(format!("invalid client project snapshot {field}"));
    }
    Ok(())
}

fn validate_route_id(value: &str, field: &str) -> Result<(), String> {
    validate_text(value, field, 256)?;
    if matches!(value, "." | "..") || value.contains(['/', '\\']) {
        return Err(format!("invalid client project snapshot {field}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> ClientProjectContextSnapshot {
        serde_json::from_value(json!({
            "schemaVersion": 1, "projectId": "project-1", "projectName": "项目", "projectRevision": 1,
            "executionTarget": {"deviceId": "device-1", "workspaceId": "workspace-1", "relativeRoot": "apps/example"}
        })).unwrap()
    }

    fn context(snapshot: &ClientProjectContextSnapshot, owner: &str) -> ProjectExecutionContext {
        ProjectContextAuthorization {
            snapshot: snapshot.clone(),
            owner_user_id: owner.into(),
            workspace_fingerprint: "binding-1".into(),
        }
        .execution_context()
        .unwrap()
    }

    #[test]
    fn desktop_wire_contract_round_trips_without_owner_or_absolute_path() {
        let snapshot = fixture();
        snapshot.validate().unwrap();
        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        assert_eq!(value["projectName"], "项目");
        let context = context(&snapshot, "alice");
        assert_eq!(context.owner_user_id, "alice");
        assert_eq!(
            context.workspace_provider,
            WorkspaceProviderKind::LocalConnector
        );
        assert_eq!(
            context.workspace.unwrap().relative_root.as_deref(),
            Some("apps/example")
        );
    }

    #[test]
    fn rejects_unknown_fields_and_owner_injection() {
        for key in [
            "ownerUserId",
            "owner_user_id",
            "rootPath",
            "workspaceProvider",
        ] {
            let mut value = serde_json::to_value(fixture()).unwrap();
            value[key] = json!("attacker");
            assert!(serde_json::from_value::<ClientProjectContextSnapshot>(value).is_err());
        }
        let mut value = serde_json::to_value(fixture()).unwrap();
        value["executionTarget"]["absoluteRoot"] = json!("/private");
        assert!(serde_json::from_value::<ClientProjectContextSnapshot>(value).is_err());
    }

    #[test]
    fn rejects_noncanonical_paths_and_identifiers() {
        for root in [
            "/root", "../root", "a/../b", "a/./b", "a//b", "a/", "C:/repo", "a\\b", " repo",
            "repo\n",
        ] {
            let mut value = fixture();
            value.execution_target.relative_root = root.to_string();
            assert!(value.validate().is_err(), "accepted {root:?}");
        }
        for id in ["", ".", "..", "a/b", "a\\b", " spaced", "bad\0"] {
            let mut value = fixture();
            value.execution_target.device_id = id.to_string();
            assert!(value.validate().is_err(), "accepted {id:?}");
        }
        let mut root = fixture();
        root.execution_target.relative_root.clear();
        root.validate().unwrap();
    }

    #[test]
    fn rejects_invalid_version_revision_and_mismatched_project() {
        for revision in [0, -1, i64::MAX] {
            let mut value = fixture();
            value.project_revision = revision;
            assert!(value.validate().is_err());
        }
        let mut value = fixture();
        value.schema_version = 2;
        assert!(value.validate().is_err());
        assert!(fixture().validate_project_id("another").is_err());
        assert!(fixture().validate_for_owner(" ").is_err());
    }

    #[test]
    fn authorization_rejects_substituted_owner_snapshot_and_binding() {
        let snapshot = fixture();
        let mut result = ProjectContextAuthorization {
            snapshot: snapshot.clone(),
            owner_user_id: "alice".into(),
            workspace_fingerprint: "binding-1".into(),
        };
        result.validate_expected("alice", &snapshot).unwrap();
        assert!(result.validate_expected("bob", &snapshot).is_err());
        result.snapshot.project_revision += 1;
        assert!(result.validate_expected("alice", &snapshot).is_err());
        result.snapshot = snapshot;
        result.workspace_fingerprint = " ".into();
        assert!(result.execution_context().is_err());
    }

    #[test]
    fn revision_binds_owner_name_project_revision_and_execution_target() {
        let snapshot = fixture();
        let original = context(&snapshot, "alice");
        assert_eq!(original, context(&snapshot, "alice"));
        assert_ne!(original.revision, context(&snapshot, "bob").revision);
        let mut variants = vec![snapshot.clone(); 6];
        variants[0].project_id = "other".into();
        variants[1].project_name = "Renamed".into();
        variants[2].project_revision += 1;
        variants[3].execution_target.device_id = "other-device".into();
        variants[4].execution_target.workspace_id = "other-workspace".into();
        variants[5].execution_target.relative_root = "another/path".into();
        for changed in variants {
            assert_ne!(original.revision, context(&changed, "alice").revision);
        }
        assert_eq!(snapshot.project_name, "项目");
    }
}

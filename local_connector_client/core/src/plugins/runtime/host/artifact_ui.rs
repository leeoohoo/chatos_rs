// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PluginRuntimeHost {
    pub(super) fn read_ui_asset(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let access = artifact_ui_access_from_body(&request.body)?;
        let relative_path = required_body_text(&request.body, "relative_path")?;
        let grant = self.artifact_store.ui_grant(request, &access, "")?;
        self.validate_current_ui_grant(&grant)?;
        let ui = &grant.ui;
        if relative_path != ui.relative_source_path
            && !ui
                .assets
                .iter()
                .any(|asset| asset.relative_path == relative_path)
        {
            return Err((
                403,
                "Plugin UI asset was not published during prepare".to_string(),
            ));
        }
        let asset = self
            .ui_loader
            .read_asset(ui, &grant.permission_snapshot, relative_path.as_str())
            .map_err(|error| (409, error.to_string()))?;
        serde_json::to_value(PluginUiAssetReadResponse {
            run_id: grant.run_id,
            owner_user_id: grant.owner_user_id,
            plugin_id: grant.plugin_id,
            release_id: grant.release_id,
            artifact_sha256: grant.artifact_sha256,
            component_key: grant.component_key,
            adapter_session_id: grant.adapter_session_id,
            ui_snapshot_sha256: access.ui_snapshot_sha256,
            kind: asset.kind,
            relative_path: asset.relative_path,
            media_type: asset.media_type,
            size_bytes: asset.size_bytes,
            sha256: asset.sha256,
            body_base64: BASE64_STANDARD.encode(asset.bytes),
        })
        .map_err(|error| internal_error(error.into()))
    }

    pub(super) fn list_artifacts(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let list: PluginArtifactListRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact list request is invalid: {error}"),
                )
            })?;
        let grant = self.artifact_store.ui_grant(
            request,
            &list.access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        )?;
        self.validate_current_ui_grant(&grant)?;
        serde_json::to_value(self.artifact_store.list(&grant, list.access)?)
            .map_err(|error| internal_error(error.into()))
    }

    pub(super) async fn read_artifact(
        &self,
        request: &RelayRequest,
    ) -> Result<Value, (u16, String)> {
        let read: PluginArtifactReadRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact read request is invalid: {error}"),
                )
            })?;
        let capability = match read.mode {
            PluginArtifactReadMode::Inline => PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
            PluginArtifactReadMode::Download => PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
        };
        let grant = self
            .artifact_store
            .ui_grant(request, &read.access, capability)?;
        self.validate_current_ui_grant(&grant)?;
        let state = self.state_snapshot().await?;
        serde_json::to_value(self.artifact_store.read(
            &state,
            request,
            &grant,
            read.access,
            read.artifact_id.as_str(),
            read.mode,
        )?)
        .map_err(|error| internal_error(error.into()))
    }

    pub(super) async fn create_artifact(
        &self,
        request: &RelayRequest,
    ) -> Result<Value, (u16, String)> {
        let create: PluginArtifactCreateRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact create request is invalid: {error}"),
                )
            })?;
        let grant = self.artifact_store.ui_grant(
            request,
            &create.access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
        )?;
        self.validate_current_ui_grant(&grant)?;
        let bytes = decode_artifact_write_body(create.body_base64.as_str())?;
        let state = self
            .approve_artifact_write(
                request,
                &grant,
                &create.access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
                create.display_name.as_str(),
                create.media_type.as_str(),
                None,
                bytes.as_slice(),
            )
            .await?;
        serde_json::to_value(self.artifact_store.create(
            &state,
            request,
            &grant,
            create.access,
            create.display_name.as_str(),
            create.media_type.as_str(),
            bytes.as_slice(),
        )?)
        .map_err(|error| internal_error(error.into()))
    }

    pub(super) async fn update_artifact(
        &self,
        request: &RelayRequest,
    ) -> Result<Value, (u16, String)> {
        let update: PluginArtifactUpdateRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact update request is invalid: {error}"),
                )
            })?;
        let grant = self.artifact_store.ui_grant(
            request,
            &update.access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
        )?;
        self.validate_current_ui_grant(&grant)?;
        let bytes = decode_artifact_write_body(update.body_base64.as_str())?;
        let state = self
            .approve_artifact_write(
                request,
                &grant,
                &update.access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
                update.artifact_id.as_str(),
                "registered-mime-type",
                Some(update.expected_sha256.as_str()),
                bytes.as_slice(),
            )
            .await?;
        serde_json::to_value(self.artifact_store.update(
            &state,
            request,
            &grant,
            update.access,
            update.artifact_id.as_str(),
            update.expected_sha256.as_str(),
            bytes.as_slice(),
        )?)
        .map_err(|error| internal_error(error.into()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn approve_artifact_write(
        &self,
        request: &RelayRequest,
        grant: &PluginUiArtifactGrant,
        access: &PluginArtifactUiAccess,
        operation: &str,
        target: &str,
        media_type: &str,
        expected_sha256: Option<&str>,
        bytes: &[u8],
    ) -> Result<LocalState, (u16, String)> {
        let state = self.state_snapshot().await?;
        let workspace = state
            .workspace_by_id(grant.workspace_id.as_str())
            .cloned()
            .ok_or_else(|| {
                (
                    409,
                    "Plugin Artifact workspace is not registered locally".to_string(),
                )
            })?;
        let workspace_root = approved_workspace_root(workspace.absolute_root.as_path())?;
        let workspace_identity =
            crate::workspace::trust::workspace_project_config_trust_fingerprint(
                workspace_root.as_path(),
            )
            .map_err(internal_error)?;
        let requested_permissions = workspace_write_permission_request(workspace_root.as_path());
        let expected_grant = GrantedPermissionProfile::from(requested_permissions.clone());
        let body_sha256 = hex::encode(Sha256::digest(bytes));
        let mut args = vec![
            operation.to_string(),
            grant.plugin_id.clone(),
            grant.component_key.clone(),
            target.to_string(),
            media_type.to_string(),
            bytes.len().to_string(),
            body_sha256.clone(),
        ];
        if let Some(expected_sha256) = expected_sha256 {
            args.push(expected_sha256.to_string());
        }
        let approval = self
            .approval_service()?
            .approve_interactive(CommandApprovalRequest {
                request_id: format!("{}:{operation}", request.request_id),
                project_key: approval_project_key_for_relay_scope(&state, request),
                command: "plugin-artifact-write".to_string(),
                args,
                redact_arguments_in_history: true,
                cwd: ".".to_string(),
                source: "plugin_artifact_write".to_string(),
                requested_permissions: Some(requested_permissions),
                session_id: Some(grant.adapter_session_id.clone()),
                action_audit: Some(ApprovalActionAudit {
                    kind: "plugin_artifact_write".to_string(),
                    operation: operation.to_string(),
                    details: vec![
                        ApprovalActionAuditDetail {
                            key: "plugin_id".to_string(),
                            value: grant.plugin_id.clone(),
                        },
                        ApprovalActionAuditDetail {
                            key: "component_key".to_string(),
                            value: grant.component_key.clone(),
                        },
                        ApprovalActionAuditDetail {
                            key: "workspace_id".to_string(),
                            value: grant.workspace_id.clone(),
                        },
                        ApprovalActionAuditDetail {
                            key: "body_size_bytes".to_string(),
                            value: bytes.len().to_string(),
                        },
                        ApprovalActionAuditDetail {
                            key: "body_sha256".to_string(),
                            value: body_sha256,
                        },
                    ],
                    privacy: Some(
                        "The approval and persistent history omit the Artifact body and redact request arguments."
                            .to_string(),
                    ),
                    safety: Some(
                        "Approval authorizes one exact UI Artifact create/update inside a Host-generated workspace path; Plugin roots, .git, network, and arbitrary paths remain unavailable."
                            .to_string(),
                    ),
                    recovery: Some(
                        "Deny to skip the write. Mutable updates require the previously registered SHA-256 and can be reviewed or reverted with the workspace's normal tools."
                            .to_string(),
                    ),
                }),
            })
            .await
            .map_err(internal_error)?;
        match approval {
            ApprovalDecision::Approved {
                granted_permissions,
                permission_scope,
                ..
            } if granted_permissions.as_ref() == Some(&expected_grant)
                && permission_scope == PermissionGrantScope::Turn => {}
            ApprovalDecision::Approved { .. } => {
                return Err((
                    403,
                    "Plugin Artifact approval did not grant the exact one-write workspace scope"
                        .to_string(),
                ));
            }
            ApprovalDecision::Denied { reason, .. } => {
                return Err((
                    403,
                    format!("Plugin Artifact write approval was denied: {reason}"),
                ));
            }
        }
        let current_grant = self.artifact_store.ui_grant(request, access, operation)?;
        if current_grant != *grant {
            return Err((
                409,
                "Plugin Artifact UI grant changed during approval".to_string(),
            ));
        }
        self.validate_current_ui_grant(&current_grant)?;
        let current_state = self.state_snapshot().await?;
        let current_workspace = current_state
            .workspace_by_id(grant.workspace_id.as_str())
            .ok_or_else(|| {
                (
                    409,
                    "Plugin Artifact workspace registration changed during approval".to_string(),
                )
            })?;
        let current_workspace_root =
            approved_workspace_root(current_workspace.absolute_root.as_path())?;
        let current_identity = crate::workspace::trust::workspace_project_config_trust_fingerprint(
            current_workspace_root.as_path(),
        )
        .map_err(internal_error)?;
        if current_workspace_root != workspace_root || current_identity != workspace_identity {
            return Err((
                409,
                "Plugin Artifact workspace registration or identity changed during approval"
                    .to_string(),
            ));
        }
        Ok(current_state)
    }

    fn validate_current_ui_grant(
        &self,
        grant: &PluginUiArtifactGrant,
    ) -> Result<(), (u16, String)> {
        if self
            .disabled_plugins
            .lock()
            .map_err(|_| {
                (
                    500,
                    "Plugin disabled-state store is unavailable".to_string(),
                )
            })?
            .contains(grant.plugin_id.as_str())
        {
            return Err((403, "Plugin is disabled by the user".to_string()));
        }
        let current = self
            .ui_loader
            .load(
                grant.plugin_id.as_str(),
                grant.component_key.as_str(),
                grant.ui.content_sha256.as_str(),
                &grant.permission_snapshot,
            )
            .map_err(|error| (409, error.to_string()))?;
        if current != grant.ui {
            return Err((
                409,
                "Plugin UI no longer matches the prepared immutable Release".to_string(),
            ));
        }
        Ok(())
    }
}

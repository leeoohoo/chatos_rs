// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use chatos_sandbox_contract::{
    legacy_policy_permission_snapshot, ActivePermissionProfile, ApprovalPolicy, ApprovalReviewer,
    CodexPermissionProfileDocument, CustomPermissionProfile, EffectivePermissionSnapshot,
    EffectiveSandboxPolicy, NetworkPermissionPolicy, NetworkRequirements, PermissionProfileId,
    PermissionProfileProvenance, PermissionProfileSummary, ResolvedPermissionProfile,
    SandboxBackendKind, SandboxLeasePolicyRequest,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::permission_layers::{
    EffectivePermissionProfileConfiguration, RuntimePermissionProfileLayers,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalSandboxState {
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) default_backend: SandboxBackendKind,
    #[serde(default)]
    pub(crate) default_permission_profile_id: PermissionProfileId,
    #[serde(default)]
    pub(crate) default_permission_profile_name: Option<String>,
    #[serde(default)]
    pub(crate) permission_profiles: BTreeMap<String, CustomPermissionProfile>,
    #[serde(default)]
    pub(crate) default_approval_policy: ApprovalPolicy,
    #[serde(default)]
    pub(crate) default_approval_reviewer: ApprovalReviewer,
    #[serde(default)]
    pub(crate) default_network_access: Option<LocalSandboxNetworkAccess>,
    #[serde(default)]
    pub(crate) default_network_requirements: NetworkRequirements,
    #[serde(default)]
    pub(crate) allowed_permission_profiles: Option<BTreeMap<String, bool>>,
    #[serde(skip)]
    pub(crate) runtime_permission_profile_layers: RuntimePermissionProfileLayers,
    #[serde(default)]
    pub(crate) policy_revision: Option<String>,
}

impl Default for LocalSandboxState {
    fn default() -> Self {
        Self {
            enabled: true,
            default_backend: SandboxBackendKind::LocalProcess,
            default_permission_profile_id: PermissionProfileId::WorkspaceWrite,
            default_permission_profile_name: None,
            permission_profiles: BTreeMap::new(),
            default_approval_policy: ApprovalPolicy::OnRequest,
            default_approval_reviewer: ApprovalReviewer::User,
            default_network_access: None,
            default_network_requirements: NetworkRequirements {
                enabled: Some(false),
                ..Default::default()
            },
            allowed_permission_profiles: None,
            runtime_permission_profile_layers: RuntimePermissionProfileLayers::default(),
            policy_revision: None,
        }
    }
}

impl LocalSandboxState {
    pub(crate) fn effective_default_network_access(&self) -> LocalSandboxNetworkAccess {
        self.default_network_access.unwrap_or_else(|| {
            if self.default_network_requirements.enabled == Some(true) {
                LocalSandboxNetworkAccess::Controlled
            } else {
                LocalSandboxNetworkAccess::Disabled
            }
        })
    }

    pub(crate) fn effective_default_network_requirements(&self) -> NetworkRequirements {
        let mut requirements = self.default_network_requirements.clone();
        match self.effective_default_network_access() {
            LocalSandboxNetworkAccess::Disabled => {
                requirements.enabled = Some(false);
            }
            LocalSandboxNetworkAccess::Controlled => {
                requirements.enabled = Some(true);
            }
            LocalSandboxNetworkAccess::Host => {}
        }
        requirements
    }

    pub(crate) fn load_runtime_permission_profile_layers(
        &mut self,
        cloud_managed: Option<CodexPermissionProfileDocument>,
    ) -> anyhow::Result<()> {
        self.runtime_permission_profile_layers =
            RuntimePermissionProfileLayers::load_from_environment_with_cloud_managed(
                cloud_managed,
            )?;
        self.effective_permission_profile_configuration()
            .map(|_| ())
            .map_err(anyhow::Error::msg)
    }

    pub(crate) fn block_runtime_permission_profile_layers(&mut self, message: impl Into<String>) {
        self.runtime_permission_profile_layers = RuntimePermissionProfileLayers::blocked(message);
    }

    pub(crate) fn effective_permission_profile_configuration(
        &self,
    ) -> Result<EffectivePermissionProfileConfiguration, String> {
        self.effective_permission_profile_configuration_with_project(None)
    }

    pub(crate) fn effective_permission_profile_configuration_with_project(
        &self,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> Result<EffectivePermissionProfileConfiguration, String> {
        self.runtime_permission_profile_layers
            .effective_configuration_with_project(
                &self.permission_profiles,
                self.allowed_permission_profiles.as_ref(),
                self.default_permission_profile_name.as_deref(),
                self.default_permission_profile_id,
                project,
            )
            .map_err(|err| err.to_string())
    }

    pub(crate) fn permission_profile_name_allowed(&self, profile_name: &str) -> bool {
        self.effective_permission_profile_configuration()
            .is_ok_and(|effective| effective.configuration.profile_allowed(profile_name))
    }

    pub(crate) fn permission_profile_catalog(&self) -> Vec<PermissionProfileSummary> {
        let Ok(effective) = self.effective_permission_profile_configuration() else {
            return PermissionProfileId::ALL
                .into_iter()
                .map(|profile| PermissionProfileSummary {
                    id: profile.codex_name().to_string(),
                    allowed: false,
                    description: None,
                })
                .collect();
        };
        effective
            .configuration
            .catalog()
            .into_iter()
            .map(|mut summary| {
                if summary.description.is_none() {
                    summary.description = PermissionProfileId::ALL
                        .into_iter()
                        .find(|profile| profile.codex_name() == summary.id)
                        .map(|profile| {
                            match profile {
                                PermissionProfileId::ReadOnly => {
                                    "Read files without workspace writes"
                                }
                                PermissionProfileId::WorkspaceWrite => {
                                    "Read the computer and write only approved workspace roots"
                                }
                                PermissionProfileId::FullAccess => {
                                    "Disable filesystem and network sandbox restrictions"
                                }
                            }
                            .to_string()
                        });
                }
                summary
            })
            .collect()
    }

    pub(crate) fn effective_default_permission_profile_name(&self) -> String {
        self.effective_default_permission_profile_name_with_project(None)
    }

    pub(crate) fn effective_default_permission_profile_name_with_project(
        &self,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> String {
        self.effective_permission_profile_configuration_with_project(project)
            .map(|effective| effective.default_profile_name)
            .unwrap_or_else(|_| PermissionProfileId::ReadOnly.codex_name().to_string())
    }

    pub(crate) fn effective_default_permission_profile(&self) -> PermissionProfileId {
        self.effective_default_permission_profile_with_project(None)
    }

    pub(crate) fn effective_default_permission_profile_with_project(
        &self,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> PermissionProfileId {
        self.resolve_permission_profile_with_project(
            self.effective_default_permission_profile_name_with_project(project)
                .as_str(),
            Vec::new(),
            project,
        )
        .map(|resolved| resolved.permission_profile_id)
        .unwrap_or(PermissionProfileId::ReadOnly)
    }

    pub(crate) fn effective_policy_defaults(&self) -> EffectiveSandboxPolicy {
        self.effective_policy_defaults_with_project(None)
    }

    pub(crate) fn effective_policy_defaults_with_project(
        &self,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> EffectiveSandboxPolicy {
        EffectiveSandboxPolicy {
            sandbox_mode: self.default_backend,
            permission_profile_id: self.effective_default_permission_profile_with_project(project),
            approval_policy: self.default_approval_policy,
            approval_reviewer: self.default_approval_reviewer,
            policy_revision: self.effective_policy_revision_with_project(project),
            additional_writable_roots: Vec::new(),
        }
    }

    pub(crate) fn effective_permissions(
        &self,
        profile_name: Option<&str>,
        policy: &EffectiveSandboxPolicy,
        runtime_workspace_roots: Vec<String>,
    ) -> EffectivePermissionSnapshot {
        self.effective_permissions_with_project(profile_name, policy, runtime_workspace_roots, None)
    }

    pub(crate) fn effective_permissions_with_project(
        &self,
        profile_name: Option<&str>,
        policy: &EffectiveSandboxPolicy,
        runtime_workspace_roots: Vec<String>,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> EffectivePermissionSnapshot {
        if let Some(profile_name) = profile_name {
            if let Ok(resolved) = self.resolve_permission_profile_with_project(
                profile_name,
                runtime_workspace_roots.clone(),
                project,
            ) {
                let mut snapshot = resolved.effective_permissions;
                self.apply_builtin_network_policy(&mut snapshot, Some(profile_name));
                return snapshot;
            }
        }
        let mut snapshot = legacy_policy_permission_snapshot(policy, runtime_workspace_roots);
        self.apply_builtin_network_policy(&mut snapshot, profile_name);
        snapshot
    }

    pub(crate) fn resolve_permission_profile(
        &self,
        profile_name: &str,
        runtime_workspace_roots: Vec<String>,
    ) -> Result<ResolvedPermissionProfile, String> {
        self.resolve_permission_profile_with_project(profile_name, runtime_workspace_roots, None)
    }

    pub(crate) fn resolve_permission_profile_with_project(
        &self,
        profile_name: &str,
        runtime_workspace_roots: Vec<String>,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> Result<ResolvedPermissionProfile, String> {
        let effective = self.effective_permission_profile_configuration_with_project(project)?;
        let provenance = if profile_name == effective.default_profile_name {
            effective.default_provenance
        } else {
            effective.provenance_for(profile_name)
        };
        effective.configuration.resolve(
            profile_name,
            runtime_workspace_roots,
            self.effective_policy_revision_with_project(project),
            provenance,
        )
    }

    pub(crate) fn effective_policy_revision(&self) -> Option<String> {
        self.effective_policy_revision_with_project(None)
    }

    pub(crate) fn effective_policy_revision_with_project(
        &self,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> Option<String> {
        self.runtime_permission_profile_layers
            .effective_policy_revision_with_project(self.policy_revision.as_deref(), project)
            .or_else(|| self.policy_revision.clone())
    }

    fn apply_builtin_network_policy(
        &self,
        snapshot: &mut EffectivePermissionSnapshot,
        profile_name: Option<&str>,
    ) {
        if profile_name.is_some_and(|name| !name.starts_with(':')) {
            return;
        }
        let network = match self.effective_default_network_access() {
            LocalSandboxNetworkAccess::Host => NetworkPermissionPolicy::Unrestricted,
            LocalSandboxNetworkAccess::Controlled | LocalSandboxNetworkAccess::Disabled => {
                let requirements = self.effective_default_network_requirements();
                if policy_network_matches_default(snapshot, &requirements) {
                    return;
                }
                NetworkPermissionPolicy::Restricted { requirements }
            }
        };

        let base_profile = snapshot.active_profile.id.clone();
        snapshot.active_profile = ActivePermissionProfile {
            id: "local:configured-network".to_string(),
            extends: Some(base_profile),
        };
        snapshot.provenance = PermissionProfileProvenance::User;
        snapshot.network = network;
    }
}

fn policy_network_matches_default(
    snapshot: &EffectivePermissionSnapshot,
    requirements: &NetworkRequirements,
) -> bool {
    matches!(
        &snapshot.network,
        NetworkPermissionPolicy::Restricted {
            requirements: current,
        } if current == requirements
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LocalSandboxNetworkAccess {
    Disabled,
    Controlled,
    Host,
}

#[derive(Clone, Default)]
pub(crate) struct LocalSandboxRuntime {
    pub(crate) leases: Arc<RwLock<HashMap<String, LocalSandboxLease>>>,
}

impl std::fmt::Debug for LocalSandboxRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalSandboxRuntime")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalSandboxLease {
    pub(crate) id: String,
    pub(crate) sandbox_id: String,
    pub(crate) tenant_id: String,
    pub(crate) user_id: String,
    pub(crate) project_id: String,
    pub(crate) run_id: String,
    pub(crate) workspace_root: String,
    pub(crate) run_workspace: String,
    pub(crate) backend: String,
    pub(crate) backend_id: Option<String>,
    pub(crate) status: String,
    pub(crate) resource_limits: LocalSandboxResourceLimits,
    pub(crate) network: LocalSandboxNetworkPolicy,
    pub(crate) tools: Vec<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) expires_at: String,
    pub(crate) destroyed_at: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) effective_policy: EffectiveSandboxPolicy,
    pub(crate) effective_permissions: EffectivePermissionSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalSandboxResourceLimits {
    pub(crate) cpu: f32,
    pub(crate) memory_mb: u64,
    pub(crate) disk_mb: u64,
    pub(crate) max_processes: u32,
}

impl Default for LocalSandboxResourceLimits {
    fn default() -> Self {
        Self {
            cpu: 2.0,
            memory_mb: 4096,
            disk_mb: 10240,
            max_processes: 128,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalSandboxNetworkPolicy {
    #[serde(default = "default_local_sandbox_network_mode")]
    pub(crate) mode: String,
}

impl Default for LocalSandboxNetworkPolicy {
    fn default() -> Self {
        Self {
            mode: default_local_sandbox_network_mode(),
        }
    }
}

fn default_local_sandbox_network_mode() -> String {
    "bridge".to_string()
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateLocalSandboxLeaseRequest {
    pub(crate) tenant_id: String,
    pub(crate) user_id: String,
    pub(crate) project_id: String,
    pub(crate) run_id: String,
    pub(crate) workspace_root: String,
    #[serde(default)]
    pub(crate) tools: Vec<String>,
    pub(crate) ttl_seconds: Option<u64>,
    pub(crate) resource_limits: Option<LocalSandboxResourceLimits>,
    pub(crate) network: Option<LocalSandboxNetworkPolicy>,
    #[serde(flatten)]
    pub(crate) policy: SandboxLeasePolicyRequest,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReleaseLocalSandboxRequest {
    pub(crate) lease_id: String,
    #[serde(default)]
    pub(crate) export_result: bool,
    #[serde(default = "default_true")]
    pub(crate) destroy: bool,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{
        FileSystemPermissionPolicy, NetworkDomainPermission, NetworkProxyMode,
    };
    use std::collections::BTreeMap;

    #[test]
    fn configured_network_policy_is_reflected_in_effective_snapshot() {
        let state = LocalSandboxState {
            default_network_requirements: NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Full),
                domains: Some(BTreeMap::from([(
                    "api.openai.com".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                ..Default::default()
            },
            ..Default::default()
        };
        let policy = state.effective_policy_defaults();
        let snapshot = state.effective_permissions(
            Some(PermissionProfileId::WorkspaceWrite.codex_name()),
            &policy,
            vec!["/workspace".to_string()],
        );

        assert_eq!(snapshot.provenance, PermissionProfileProvenance::User);
        assert_eq!(
            snapshot.active_profile.extends.as_deref(),
            Some(":workspace")
        );
        let NetworkPermissionPolicy::Restricted { requirements } = snapshot.network else {
            panic!("configured profile must retain restricted networking");
        };
        assert_eq!(requirements.enabled, Some(true));
        assert_eq!(
            requirements
                .domains
                .as_ref()
                .and_then(|domains| domains.get("api.openai.com")),
            Some(&NetworkDomainPermission::Allow)
        );
    }

    #[test]
    fn full_access_file_profile_keeps_network_separate() {
        let state = LocalSandboxState {
            default_permission_profile_id: PermissionProfileId::FullAccess,
            default_network_access: Some(LocalSandboxNetworkAccess::Disabled),
            default_network_requirements: NetworkRequirements {
                enabled: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        let policy = state.effective_policy_defaults();
        let snapshot = state.effective_permissions(
            Some(PermissionProfileId::FullAccess.codex_name()),
            &policy,
            vec!["/workspace".to_string()],
        );

        assert!(matches!(
            snapshot.file_system,
            FileSystemPermissionPolicy::Unrestricted
        ));
        let NetworkPermissionPolicy::Restricted { requirements } = snapshot.network else {
            panic!("network must remain controlled independently from file access");
        };
        assert_eq!(requirements.enabled, Some(false));
    }

    #[test]
    fn host_network_is_independent_from_file_profile() {
        let state = LocalSandboxState {
            default_permission_profile_id: PermissionProfileId::WorkspaceWrite,
            default_network_access: Some(LocalSandboxNetworkAccess::Host),
            ..Default::default()
        };
        let policy = state.effective_policy_defaults();
        let snapshot = state.effective_permissions(
            Some(PermissionProfileId::WorkspaceWrite.codex_name()),
            &policy,
            vec!["/workspace".to_string()],
        );

        assert!(matches!(
            snapshot.file_system,
            FileSystemPermissionPolicy::Restricted { .. }
        ));
        assert!(matches!(
            snapshot.network,
            NetworkPermissionPolicy::Unrestricted
        ));
    }

    #[test]
    fn managed_permission_profile_allowlist_is_complete_and_fail_closed() {
        let state = LocalSandboxState {
            default_permission_profile_id: PermissionProfileId::FullAccess,
            allowed_permission_profiles: Some(BTreeMap::from([
                (":read-only".to_string(), true),
                (":workspace".to_string(), true),
                (":danger-full-access".to_string(), false),
            ])),
            ..Default::default()
        };

        assert_eq!(
            state.effective_default_permission_profile(),
            PermissionProfileId::ReadOnly
        );
        let catalog = state.permission_profile_catalog();
        assert_eq!(catalog.len(), 3);
        assert!(
            !catalog
                .iter()
                .find(|entry| entry.id == ":danger-full-access")
                .expect("full access")
                .allowed
        );
    }

    #[test]
    fn managed_default_profile_retains_managed_provenance() {
        let managed = chatos_sandbox_contract::parse_codex_permission_profile_toml(
            r#"
default_permissions = "acme-review"

[allowed_permission_profiles]
acme-review = true

[permissions.acme-review]
extends = ":read-only"
"#,
        )
        .expect("parse managed profile");
        let state = LocalSandboxState {
            runtime_permission_profile_layers: RuntimePermissionProfileLayers::for_tests(
                None,
                None,
                Some(managed),
            ),
            ..Default::default()
        };

        let resolved = state
            .resolve_permission_profile(
                state.effective_default_permission_profile_name().as_str(),
                Vec::new(),
            )
            .expect("resolve managed default");

        assert_eq!(resolved.profile_name, "acme-review");
        assert_eq!(
            resolved.effective_permissions.provenance,
            PermissionProfileProvenance::Managed
        );
        assert!(state
            .permission_profile_catalog()
            .iter()
            .any(|profile| profile.id == "acme-review" && profile.allowed));
    }

    #[test]
    fn runtime_permission_layers_are_not_persisted_in_user_state() {
        let managed = chatos_sandbox_contract::parse_codex_permission_profile_toml(
            r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
"#,
        )
        .expect("parse managed profile");
        let state = LocalSandboxState {
            runtime_permission_profile_layers: RuntimePermissionProfileLayers::for_tests(
                None,
                None,
                Some(managed),
            ),
            ..Default::default()
        };

        let serialized = serde_json::to_value(&state).expect("serialize sandbox state");

        assert!(serialized
            .get("runtime_permission_profile_layers")
            .is_none());
        assert!(serialized.get("runtimePermissionProfileLayers").is_none());
    }
}

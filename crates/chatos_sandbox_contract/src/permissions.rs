// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EffectiveSandboxPolicy, PermissionProfileId};

/// Identifies the configuration layer that produced an active permission profile.
///
/// Codex currently exposes `id` and `extends` on its public app-server protocol. ChatOS keeps
/// provenance in the effective snapshot as well so managed, external, and disabled profiles
/// cannot be presented as an ordinary user-selected profile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionProfileProvenance {
    #[default]
    BuiltIn,
    User,
    Project,
    Managed,
    External,
    Disabled,
}

/// Public profile identity matching Codex's `ActivePermissionProfile` shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePermissionProfile {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionProfileSummary {
    pub id: String,
    pub allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileSystemAccessMode {
    Read,
    Write,
    Deny,
}

impl FileSystemAccessMode {
    pub const fn rank(self) -> u8 {
        match self {
            Self::Deny => 0,
            Self::Read => 1,
            Self::Write => 2,
        }
    }

    pub const fn is_no_broader_than(self, maximum: Self) -> bool {
        self.rank() <= maximum.rank()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileSystemSpecialPath {
    Root,
    Minimal,
    ProjectRoots {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subpath: Option<String>,
    },
    Tmpdir,
    SlashTmp,
    Unknown {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subpath: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileSystemPath {
    Path { path: String },
    GlobPattern { pattern: String },
    Special { value: FileSystemSpecialPath },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemSandboxEntry {
    pub access: FileSystemAccessMode,
    pub path: FileSystemPath,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalFileSystemPermissions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<FileSystemSandboxEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob_scan_max_depth: Option<usize>,
    /// Legacy Codex compatibility field. New callers should use `entries`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<Vec<String>>,
    /// Legacy Codex compatibility field. New callers should use `entries`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<Vec<String>>,
}

impl AdditionalFileSystemPermissions {
    pub fn is_empty(&self) -> bool {
        self.entries.as_ref().is_none_or(Vec::is_empty)
            && self.read.as_ref().is_none_or(Vec::is_empty)
            && self.write.as_ref().is_none_or(Vec::is_empty)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.glob_scan_max_depth == Some(0) {
            return Err("globScanMaxDepth must be at least 1".to_string());
        }
        for entry in self.entries.as_deref().unwrap_or_default() {
            if matches!(entry.path, FileSystemPath::GlobPattern { .. })
                && entry.access != FileSystemAccessMode::Deny
            {
                return Err("glob file system permissions only support deny entries".to_string());
            }
            validate_filesystem_path(&entry.path)?;
        }
        for path in self
            .read
            .as_deref()
            .unwrap_or_default()
            .iter()
            .chain(self.write.as_deref().unwrap_or_default())
        {
            validate_non_empty_path(path)?;
        }
        Ok(())
    }

    pub fn normalized_entries(&self) -> Vec<FileSystemSandboxEntry> {
        let mut entries = self.entries.clone().unwrap_or_default();
        entries.extend(
            self.read
                .as_deref()
                .unwrap_or_default()
                .iter()
                .cloned()
                .map(|path| FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Path { path },
                }),
        );
        entries.extend(
            self.write
                .as_deref()
                .unwrap_or_default()
                .iter()
                .cloned()
                .map(|path| FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path { path },
                }),
        );
        entries
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalNetworkPermissions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestPermissionProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_system: Option<AdditionalFileSystemPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<AdditionalNetworkPermissions>,
}

impl RequestPermissionProfile {
    pub fn is_empty(&self) -> bool {
        self.file_system
            .as_ref()
            .is_none_or(AdditionalFileSystemPermissions::is_empty)
            && self.network.as_ref().and_then(|network| network.enabled) != Some(true)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(file_system) = &self.file_system {
            file_system.validate()?;
        }
        if self.is_empty() {
            return Err("permission request does not contain an access elevation".to_string());
        }
        Ok(())
    }

    /// Ensures an approval response cannot silently grant more than the command requested.
    pub fn allows_grant(&self, grant: &GrantedPermissionProfile) -> bool {
        let requested_network = self.network.as_ref().and_then(|value| value.enabled);
        let granted_network = grant.network.as_ref().and_then(|value| value.enabled);
        if granted_network == Some(true) && requested_network != Some(true) {
            return false;
        }

        let requested_entries = self
            .file_system
            .as_ref()
            .map(AdditionalFileSystemPermissions::normalized_entries)
            .unwrap_or_default();
        let granted_entries = grant
            .file_system
            .as_ref()
            .map(AdditionalFileSystemPermissions::normalized_entries)
            .unwrap_or_default();
        if !granted_entries.iter().all(|granted| {
            requested_entries.iter().any(|requested| {
                granted.path == requested.path
                    && granted.access.is_no_broader_than(requested.access)
            })
        }) {
            return false;
        }

        let granted_has_access = granted_entries
            .iter()
            .any(|entry| entry.access != FileSystemAccessMode::Deny);
        if granted_has_access {
            let granted_denies = granted_entries
                .iter()
                .filter(|entry| entry.access == FileSystemAccessMode::Deny)
                .collect::<Vec<_>>();
            if requested_entries
                .iter()
                .filter(|entry| entry.access == FileSystemAccessMode::Deny)
                .any(|requested_deny| !granted_denies.contains(&requested_deny))
            {
                return false;
            }

            let requested_has_glob_deny = requested_entries.iter().any(|entry| {
                entry.access == FileSystemAccessMode::Deny
                    && matches!(entry.path, FileSystemPath::GlobPattern { .. })
            });
            if requested_has_glob_deny {
                let requested_depth = self
                    .file_system
                    .as_ref()
                    .and_then(|file_system| file_system.glob_scan_max_depth);
                let granted_depth = grant
                    .file_system
                    .as_ref()
                    .and_then(|file_system| file_system.glob_scan_max_depth);
                let depth_is_no_broader = match (requested_depth, granted_depth) {
                    (None, None) => true,
                    (None, Some(_)) => false,
                    (Some(_), None) => true,
                    (Some(requested), Some(granted)) => granted >= requested,
                };
                if !depth_is_no_broader {
                    return false;
                }
            }
        }
        true
    }
}

/// Partial overlay attached to a command approval request.
pub type AdditionalPermissionProfile = RequestPermissionProfile;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantedPermissionProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_system: Option<AdditionalFileSystemPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<AdditionalNetworkPermissions>,
}

impl From<RequestPermissionProfile> for GrantedPermissionProfile {
    fn from(value: RequestPermissionProfile) -> Self {
        Self {
            file_system: value.file_system,
            network: value.network,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionGrantScope {
    #[default]
    Turn,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsRequestApprovalResponse {
    pub permissions: GrantedPermissionProfile,
    #[serde(default)]
    pub scope: PermissionGrantScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict_auto_review: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkPolicyRuleAction {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPolicyAmendment {
    pub action: NetworkPolicyRuleAction,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecpolicyAmendment {
    pub execpolicy_amendment: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPolicyAmendmentDecision {
    pub network_policy_amendment: NetworkPolicyAmendment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SimpleCommandExecutionApprovalDecision {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

/// Matches the current Codex app-server `CommandExecutionApprovalDecision` union.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandExecutionApprovalDecision {
    Simple(SimpleCommandExecutionApprovalDecision),
    AcceptWithExecpolicyAmendment {
        #[serde(rename = "acceptWithExecpolicyAmendment")]
        value: ExecpolicyAmendment,
    },
    ApplyNetworkPolicyAmendment {
        #[serde(rename = "applyNetworkPolicyAmendment")]
        value: NetworkPolicyAmendmentDecision,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkDomainPermission {
    Allow,
    Deny,
}

pub type NetworkUnixSocketPermission = NetworkDomainPermission;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkAccess {
    Restricted,
    Enabled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkProxyMode {
    /// Read-only HTTP access. HTTPS CONNECT and SOCKS tunnelling require MITM support and must
    /// fail closed when the runtime cannot inspect the inner request method.
    Limited,
    /// Full HTTP method and TCP tunnelling support, still constrained by domain and destination
    /// policy.
    #[default]
    Full,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRequirements {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domains: Option<BTreeMap<String, NetworkDomainPermission>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix_sockets: Option<BTreeMap<String, NetworkUnixSocketPermission>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_local_binding: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_upstream_proxy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<NetworkProxyMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_socks5: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_socks5_udp: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dangerously_allow_all_unix_sockets: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dangerously_allow_non_loopback_proxy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_allowed_domains_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socks_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied_domains: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_unix_sockets: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileSystemPermissionPolicy {
    Restricted {
        #[serde(default)]
        entries: Vec<FileSystemSandboxEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        glob_scan_max_depth: Option<usize>,
    },
    Unrestricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NetworkPermissionPolicy {
    Restricted {
        #[serde(default)]
        requirements: NetworkRequirements,
    },
    Unrestricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectivePermissionSnapshot {
    pub active_profile: ActivePermissionProfile,
    pub provenance: PermissionProfileProvenance,
    pub file_system: FileSystemPermissionPolicy,
    pub network: NetworkPermissionPolicy,
    #[serde(default)]
    pub runtime_workspace_roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_revision: Option<String>,
}

pub fn legacy_policy_permission_snapshot(
    policy: &EffectiveSandboxPolicy,
    runtime_workspace_roots: Vec<String>,
) -> EffectivePermissionSnapshot {
    let active_profile = ActivePermissionProfile {
        id: match policy.permission_profile_id {
            PermissionProfileId::ReadOnly => ":read-only",
            PermissionProfileId::WorkspaceWrite => ":workspace",
            PermissionProfileId::FullAccess => ":danger-full-access",
        }
        .to_string(),
        extends: None,
    };
    let (file_system, network) = match policy.permission_profile_id {
        PermissionProfileId::ReadOnly => (
            FileSystemPermissionPolicy::Restricted {
                entries: vec![FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                }],
                glob_scan_max_depth: None,
            },
            NetworkPermissionPolicy::Restricted {
                requirements: NetworkRequirements {
                    enabled: Some(false),
                    ..NetworkRequirements::default()
                },
            },
        ),
        PermissionProfileId::WorkspaceWrite => {
            let mut entries = vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                    },
                },
            ];
            entries.extend([".git", ".agents", ".codex"].into_iter().map(|subpath| {
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots {
                            subpath: Some(subpath.to_string()),
                        },
                    },
                }
            }));
            entries.extend(
                policy
                    .additional_writable_roots
                    .iter()
                    .cloned()
                    .map(|path| FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Write,
                        path: FileSystemPath::Path { path },
                    }),
            );
            (
                FileSystemPermissionPolicy::Restricted {
                    entries,
                    glob_scan_max_depth: None,
                },
                NetworkPermissionPolicy::Restricted {
                    requirements: NetworkRequirements {
                        enabled: Some(false),
                        ..NetworkRequirements::default()
                    },
                },
            )
        }
        PermissionProfileId::FullAccess => (
            FileSystemPermissionPolicy::Unrestricted,
            NetworkPermissionPolicy::Unrestricted,
        ),
    };
    EffectivePermissionSnapshot {
        active_profile,
        provenance: PermissionProfileProvenance::BuiltIn,
        file_system,
        network,
        runtime_workspace_roots,
        policy_revision: policy.policy_revision.clone(),
    }
}

fn validate_filesystem_path(path: &FileSystemPath) -> Result<(), String> {
    match path {
        FileSystemPath::Path { path } => validate_non_empty_path(path),
        FileSystemPath::GlobPattern { pattern } => validate_non_empty_path(pattern),
        FileSystemPath::Special { value } => match value {
            FileSystemSpecialPath::ProjectRoots {
                subpath: Some(path),
            }
            | FileSystemSpecialPath::Unknown {
                path,
                subpath: None,
            } => validate_non_empty_path(path),
            FileSystemSpecialPath::Unknown {
                path,
                subpath: Some(subpath),
            } => {
                validate_non_empty_path(path)?;
                validate_non_empty_path(subpath)
            }
            _ => Ok(()),
        },
    }
}

fn validate_non_empty_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("filesystem permission path must not be empty".to_string());
    }
    if path.contains('\0') {
        return Err("filesystem permission path must not contain NUL".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_permission_request_shape_round_trips() {
        let request: RequestPermissionProfile = serde_json::from_value(json!({
            "fileSystem": {
                "entries": [
                    {
                        "access": "write",
                        "path": { "type": "path", "path": "/tmp/output" }
                    },
                    {
                        "access": "deny",
                        "path": {
                            "type": "special",
                            "value": { "kind": "project_roots", "subpath": ".git" }
                        }
                    }
                ],
                "globScanMaxDepth": 3
            },
            "network": { "enabled": true }
        }))
        .expect("deserialize request");

        request.validate().expect("valid request");
        assert_eq!(
            serde_json::to_value(request).expect("serialize request"),
            json!({
                "fileSystem": {
                    "entries": [
                        {
                            "access": "write",
                            "path": { "type": "path", "path": "/tmp/output" }
                        },
                        {
                            "access": "deny",
                            "path": {
                                "type": "special",
                                "value": { "kind": "project_roots", "subpath": ".git" }
                            }
                        }
                    ],
                    "globScanMaxDepth": 3
                },
                "network": { "enabled": true }
            })
        );
    }

    #[test]
    fn empty_permission_request_fails_closed() {
        let request = RequestPermissionProfile::default();
        assert!(request.validate().is_err());
    }

    #[test]
    fn grant_cannot_exceed_request() {
        let request = RequestPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Path {
                        path: "/tmp/input".to_string(),
                    },
                }]),
                ..AdditionalFileSystemPermissions::default()
            }),
            network: None,
        };
        let exact = GrantedPermissionProfile {
            file_system: request.file_system.clone(),
            network: None,
        };
        assert!(request.allows_grant(&exact));

        let broader = GrantedPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: "/tmp/input".to_string(),
                    },
                }]),
                ..AdditionalFileSystemPermissions::default()
            }),
            network: Some(AdditionalNetworkPermissions {
                enabled: Some(true),
            }),
        };
        assert!(!request.allows_grant(&broader));
    }

    #[test]
    fn grant_must_retain_requested_deny_carveouts() {
        let request = RequestPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![
                    FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Write,
                        path: FileSystemPath::Path {
                            path: "/tmp/output".to_string(),
                        },
                    },
                    FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Deny,
                        path: FileSystemPath::GlobPattern {
                            pattern: "/tmp/output/**/*.env".to_string(),
                        },
                    },
                ]),
                glob_scan_max_depth: Some(3),
                ..Default::default()
            }),
            network: None,
        };
        let missing_deny = GrantedPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: "/tmp/output".to_string(),
                    },
                }]),
                ..Default::default()
            }),
            network: None,
        };
        assert!(!request.allows_grant(&missing_deny));

        let shallower_deny = GrantedPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: request
                    .file_system
                    .as_ref()
                    .and_then(|file_system| file_system.entries.clone()),
                glob_scan_max_depth: Some(1),
                ..Default::default()
            }),
            network: None,
        };
        assert!(!request.allows_grant(&shallower_deny));
        assert!(request.allows_grant(&request.clone().into()));
    }

    #[test]
    fn command_decisions_match_codex_union_shape() {
        let session: CommandExecutionApprovalDecision =
            serde_json::from_value(json!("acceptForSession")).expect("session decision");
        assert_eq!(
            session,
            CommandExecutionApprovalDecision::Simple(
                SimpleCommandExecutionApprovalDecision::AcceptForSession
            )
        );

        let amendment: CommandExecutionApprovalDecision = serde_json::from_value(json!({
            "acceptWithExecpolicyAmendment": {
                "execpolicy_amendment": ["git", "status"]
            }
        }))
        .expect("amendment decision");
        assert!(matches!(
            amendment,
            CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment { .. }
        ));
    }

    #[test]
    fn workspace_legacy_policy_exposes_active_profile_and_protected_paths() {
        let policy = EffectiveSandboxPolicy {
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            additional_writable_roots: vec!["/tmp/shared".to_string()],
            ..EffectiveSandboxPolicy::default()
        };
        let snapshot = legacy_policy_permission_snapshot(&policy, vec!["/workspace".to_string()]);

        assert_eq!(snapshot.active_profile.id, ":workspace");
        assert_eq!(snapshot.provenance, PermissionProfileProvenance::BuiltIn);
        let FileSystemPermissionPolicy::Restricted { entries, .. } = snapshot.file_system else {
            panic!("workspace profile must stay restricted");
        };
        assert!(entries.iter().any(|entry| {
            entry.access == FileSystemAccessMode::Read
                && entry.path
                    == FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots {
                            subpath: Some(".git".to_string()),
                        },
                    }
        }));
        assert_eq!(snapshot.runtime_workspace_roots, vec!["/workspace"]);
    }
}

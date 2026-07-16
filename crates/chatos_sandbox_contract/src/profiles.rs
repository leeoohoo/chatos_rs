// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::{
    legacy_policy_permission_snapshot, ActivePermissionProfile, AdditionalFileSystemPermissions,
    EffectivePermissionSnapshot, EffectiveSandboxPolicy, FileSystemAccessMode, FileSystemPath,
    FileSystemPermissionPolicy, FileSystemSpecialPath, NetworkPermissionPolicy,
    NetworkRequirements, PermissionProfileId, PermissionProfileProvenance,
    PermissionProfileSummary,
};

const MAX_PROFILE_INHERITANCE_DEPTH: usize = 32;

/// A safe, serializable subset of Codex permission profile configuration.
///
/// Profiles may be independent or extend `:read-only`, `:workspace`, or another custom profile.
/// Independent profiles start with an empty restricted filesystem policy and disabled network,
/// which allows `:minimal` to form a genuinely narrow executable baseline. Extending
/// `:danger-full-access` is intentionally rejected because it cannot be narrowed reliably after
/// the native sandbox has already been disabled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomPermissionProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspace_roots: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_system: Option<AdditionalFileSystemPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkRequirements>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionProfileConfiguration {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, CustomPermissionProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_permission_profiles: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPermissionProfile {
    pub profile_name: String,
    pub permission_profile_id: PermissionProfileId,
    pub description: Option<String>,
    pub effective_permissions: EffectivePermissionSnapshot,
    pub profile_workspace_roots: Vec<String>,
}

impl PermissionProfileConfiguration {
    pub fn validate(&self) -> Result<(), String> {
        for (name, profile) in &self.profiles {
            validate_custom_profile_name(name)?;
            validate_custom_profile(name, profile)?;
        }
        if let Some(allowed) = &self.allowed_permission_profiles {
            if allowed.is_empty() || !allowed.values().any(|enabled| *enabled) {
                return Err(
                    "allowedPermissionProfiles must enable at least one permission profile"
                        .to_string(),
                );
            }
            for name in allowed.keys() {
                if builtin_profile_id(name).is_none() && !self.profiles.contains_key(name) {
                    return Err(format!(
                        "allowedPermissionProfiles references unknown profile {name:?}"
                    ));
                }
            }
        }
        for name in self.profiles.keys() {
            self.resolve(name, Vec::new(), None, PermissionProfileProvenance::User)?;
        }
        Ok(())
    }

    pub fn profile_allowed(&self, name: &str) -> bool {
        self.allowed_permission_profiles
            .as_ref()
            .is_none_or(|allowed| allowed.get(name).copied().unwrap_or(false))
    }

    pub fn catalog(&self) -> Vec<PermissionProfileSummary> {
        let mut catalog = PermissionProfileId::ALL
            .into_iter()
            .map(|profile| PermissionProfileSummary {
                id: profile.codex_name().to_string(),
                allowed: self.profile_allowed(profile.codex_name()),
                description: None,
            })
            .collect::<Vec<_>>();
        catalog.extend(self.profiles.iter().map(|(name, profile)| {
            let valid = self
                .resolve(name, Vec::new(), None, PermissionProfileProvenance::User)
                .is_ok();
            PermissionProfileSummary {
                id: name.clone(),
                allowed: valid && self.profile_allowed(name),
                description: profile.description.clone(),
            }
        }));
        catalog
    }

    pub fn resolve(
        &self,
        name: &str,
        runtime_workspace_roots: Vec<String>,
        policy_revision: Option<String>,
        provenance: PermissionProfileProvenance,
    ) -> Result<ResolvedPermissionProfile, String> {
        let mut stack = Vec::new();
        let mut resolved = self.resolve_inner(name, &mut stack)?;
        resolved.effective_permissions.runtime_workspace_roots = runtime_workspace_roots;
        resolved
            .effective_permissions
            .runtime_workspace_roots
            .extend(resolved.profile_workspace_roots.iter().cloned());
        deduplicate_strings(&mut resolved.effective_permissions.runtime_workspace_roots);
        resolved.effective_permissions.policy_revision = policy_revision;
        resolved.effective_permissions.provenance = provenance;
        Ok(resolved)
    }

    fn resolve_inner(
        &self,
        name: &str,
        stack: &mut Vec<String>,
    ) -> Result<ResolvedPermissionProfile, String> {
        if stack.len() >= MAX_PROFILE_INHERITANCE_DEPTH {
            return Err(format!(
                "permission profile inheritance exceeds {MAX_PROFILE_INHERITANCE_DEPTH} levels"
            ));
        }
        if let Some(profile_id) = builtin_profile_id(name) {
            let policy = EffectiveSandboxPolicy {
                permission_profile_id: profile_id,
                ..EffectiveSandboxPolicy::default()
            };
            return Ok(ResolvedPermissionProfile {
                profile_name: name.to_string(),
                permission_profile_id: profile_id,
                description: None,
                effective_permissions: legacy_policy_permission_snapshot(&policy, Vec::new()),
                profile_workspace_roots: Vec::new(),
            });
        }
        validate_custom_profile_name(name)?;
        if stack.iter().any(|entry| entry == name) {
            let mut cycle = stack.clone();
            cycle.push(name.to_string());
            return Err(format!(
                "permission profile inheritance cycle: {}",
                cycle.join(" -> ")
            ));
        }
        let profile = self
            .profiles
            .get(name)
            .ok_or_else(|| format!("unknown permission profile {name:?}"))?;
        validate_custom_profile(name, profile)?;
        if profile.extends.as_deref() == Some(PermissionProfileId::FullAccess.codex_name()) {
            return Err(
                "custom profiles cannot extend :danger-full-access because the native sandbox would already be disabled"
                    .to_string(),
            );
        }

        let mut parent = if let Some(extends) = profile.extends.as_deref() {
            stack.push(name.to_string());
            let parent = self.resolve_inner(extends, stack)?;
            stack.pop();
            parent
        } else {
            ResolvedPermissionProfile {
                profile_name: name.to_string(),
                permission_profile_id: PermissionProfileId::ReadOnly,
                description: None,
                effective_permissions: EffectivePermissionSnapshot {
                    active_profile: ActivePermissionProfile {
                        id: name.to_string(),
                        extends: None,
                    },
                    provenance: PermissionProfileProvenance::User,
                    file_system: FileSystemPermissionPolicy::Restricted {
                        entries: Vec::new(),
                        glob_scan_max_depth: None,
                    },
                    network: NetworkPermissionPolicy::Restricted {
                        requirements: NetworkRequirements {
                            enabled: Some(false),
                            ..NetworkRequirements::default()
                        },
                    },
                    runtime_workspace_roots: Vec::new(),
                    policy_revision: None,
                },
                profile_workspace_roots: Vec::new(),
            }
        };

        let file_system = merge_file_system_policy(
            parent.effective_permissions.file_system,
            profile.file_system.as_ref(),
        )?;
        let network = merge_network_policy(
            parent.effective_permissions.network,
            profile.network.as_ref(),
        );
        let mut workspace_roots = parent.profile_workspace_roots;
        for (path, enabled) in &profile.workspace_roots {
            workspace_roots.retain(|candidate| candidate != path);
            if *enabled {
                workspace_roots.push(path.clone());
            }
        }
        deduplicate_strings(&mut workspace_roots);

        let permission_profile_id = classify_file_system_policy(&file_system)?;
        parent.profile_name = name.to_string();
        parent.permission_profile_id = permission_profile_id;
        parent.description = profile.description.clone();
        parent.profile_workspace_roots = workspace_roots;
        parent.effective_permissions.active_profile = ActivePermissionProfile {
            id: name.to_string(),
            extends: profile.extends.clone(),
        };
        parent.effective_permissions.file_system = file_system;
        parent.effective_permissions.network = network;
        Ok(parent)
    }
}

fn validate_custom_profile_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("custom permission profile name must not be empty".to_string());
    }
    if name.starts_with(':') {
        return Err(format!(
            "custom permission profile {name:?} uses the reserved built-in prefix"
        ));
    }
    if name.eq_ignore_ascii_case("filesystem") {
        return Err("custom permission profile name `filesystem` is reserved".to_string());
    }
    Ok(())
}

fn validate_custom_profile(name: &str, profile: &CustomPermissionProfile) -> Result<(), String> {
    if profile
        .extends
        .as_deref()
        .is_some_and(|extends| extends.trim().is_empty())
    {
        return Err(format!(
            "custom permission profile {name:?} extends must not be empty"
        ));
    }
    for (path, enabled) in &profile.workspace_roots {
        if *enabled {
            validate_profile_workspace_root(path)?;
        }
    }
    if let Some(file_system) = &profile.file_system {
        file_system.validate()?;
        if file_system.glob_scan_max_depth == Some(0) {
            return Err("globScanMaxDepth must be at least 1".to_string());
        }
        for entry in file_system.normalized_entries() {
            validate_profile_file_system_entry(&entry.path, entry.access)?;
        }
        let has_unbounded_deny = file_system.normalized_entries().iter().any(|entry| {
            entry.access == FileSystemAccessMode::Deny
                && matches!(&entry.path, FileSystemPath::GlobPattern { pattern } if pattern.contains("**"))
        });
        if has_unbounded_deny && file_system.glob_scan_max_depth.is_none() {
            return Err(
                "custom profiles with an unbounded ** deny glob must set globScanMaxDepth"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn validate_profile_workspace_root(path: &str) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() || path.contains('\0') {
        return Err("profile workspace root must be a non-empty path".to_string());
    }
    if is_absolute_or_home_path(path) {
        Ok(())
    } else {
        Err(format!(
            "profile workspace root must be absolute or home-relative: {path:?}"
        ))
    }
}

fn validate_profile_file_system_entry(
    path: &FileSystemPath,
    access: FileSystemAccessMode,
) -> Result<(), String> {
    match path {
        FileSystemPath::Path { path } => {
            if !is_absolute_or_home_path(path) {
                return Err(format!(
                    "custom profile filesystem path must be absolute or home-relative: {path:?}"
                ));
            }
        }
        FileSystemPath::GlobPattern { pattern } => {
            if access != FileSystemAccessMode::Deny {
                return Err("custom profile globs only support deny access".to_string());
            }
            if Path::new(pattern).is_absolute() || pattern.starts_with("~/") {
                return Ok(());
            }
            if has_parent_component(pattern) {
                return Err(format!(
                    "custom profile deny glob must stay below its workspace root: {pattern:?}"
                ));
            }
        }
        FileSystemPath::Special { value } => match value {
            FileSystemSpecialPath::Root if access != FileSystemAccessMode::Read => {
                return Err("custom profiles may only grant read access to :root".to_string())
            }
            FileSystemSpecialPath::Minimal if access != FileSystemAccessMode::Read => {
                return Err("custom profiles may only grant read access to :minimal".to_string())
            }
            FileSystemSpecialPath::ProjectRoots {
                subpath: Some(subpath),
            } if has_parent_component(subpath) => {
                return Err(format!(
                    "workspace-root subpath must not contain parent traversal: {subpath:?}"
                ))
            }
            FileSystemSpecialPath::Unknown { .. } => {
                return Err("unknown special filesystem paths are not executable yet".to_string())
            }
            _ => {}
        },
    }
    Ok(())
}

fn merge_file_system_policy(
    parent: FileSystemPermissionPolicy,
    child: Option<&AdditionalFileSystemPermissions>,
) -> Result<FileSystemPermissionPolicy, String> {
    let FileSystemPermissionPolicy::Restricted {
        mut entries,
        glob_scan_max_depth,
    } = parent
    else {
        return Err("custom profiles cannot narrow an unrestricted filesystem profile".to_string());
    };
    let Some(child) = child else {
        return Ok(FileSystemPermissionPolicy::Restricted {
            entries,
            glob_scan_max_depth,
        });
    };
    for entry in child.normalized_entries() {
        entries.retain(|existing| existing.path != entry.path);
        entries.push(entry);
    }
    Ok(FileSystemPermissionPolicy::Restricted {
        entries,
        glob_scan_max_depth: child.glob_scan_max_depth.or(glob_scan_max_depth),
    })
}

fn merge_network_policy(
    parent: NetworkPermissionPolicy,
    child: Option<&NetworkRequirements>,
) -> NetworkPermissionPolicy {
    let Some(child) = child else {
        return parent;
    };
    let parent = match parent {
        NetworkPermissionPolicy::Restricted { requirements } => requirements,
        NetworkPermissionPolicy::Unrestricted => NetworkRequirements::default(),
    };
    NetworkPermissionPolicy::Restricted {
        requirements: merge_network_requirements(parent, child.clone()),
    }
}

fn merge_network_requirements(
    parent: NetworkRequirements,
    child: NetworkRequirements,
) -> NetworkRequirements {
    let mut domains = parent.domains.unwrap_or_default();
    domains.extend(child.domains.clone().unwrap_or_default());
    let mut unix_sockets = parent.unix_sockets.unwrap_or_default();
    unix_sockets.extend(child.unix_sockets.clone().unwrap_or_default());
    NetworkRequirements {
        enabled: child.enabled.or(parent.enabled),
        domains: (!domains.is_empty()).then_some(domains),
        unix_sockets: (!unix_sockets.is_empty()).then_some(unix_sockets),
        allow_local_binding: child.allow_local_binding.or(parent.allow_local_binding),
        allow_upstream_proxy: child.allow_upstream_proxy.or(parent.allow_upstream_proxy),
        mode: child.mode.or(parent.mode),
        enable_socks5: child.enable_socks5.or(parent.enable_socks5),
        enable_socks5_udp: child.enable_socks5_udp.or(parent.enable_socks5_udp),
        dangerously_allow_all_unix_sockets: child
            .dangerously_allow_all_unix_sockets
            .or(parent.dangerously_allow_all_unix_sockets),
        dangerously_allow_non_loopback_proxy: child
            .dangerously_allow_non_loopback_proxy
            .or(parent.dangerously_allow_non_loopback_proxy),
        managed_allowed_domains_only: child
            .managed_allowed_domains_only
            .or(parent.managed_allowed_domains_only),
        http_port: child.http_port.or(parent.http_port),
        socks_port: child.socks_port.or(parent.socks_port),
        allowed_domains: child.allowed_domains.or(parent.allowed_domains),
        denied_domains: child.denied_domains.or(parent.denied_domains),
        allow_unix_sockets: child.allow_unix_sockets.or(parent.allow_unix_sockets),
    }
}

fn classify_file_system_policy(
    policy: &FileSystemPermissionPolicy,
) -> Result<PermissionProfileId, String> {
    let FileSystemPermissionPolicy::Restricted { entries, .. } = policy else {
        return Ok(PermissionProfileId::FullAccess);
    };
    if entries.iter().any(|entry| {
        entry.access == FileSystemAccessMode::Write
            && matches!(
                entry.path,
                FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root
                }
            )
    }) {
        return Err("custom profiles cannot grant write access to :root".to_string());
    }
    Ok(
        if entries
            .iter()
            .any(|entry| entry.access == FileSystemAccessMode::Write)
        {
            PermissionProfileId::WorkspaceWrite
        } else {
            PermissionProfileId::ReadOnly
        },
    )
}

fn builtin_profile_id(name: &str) -> Option<PermissionProfileId> {
    PermissionProfileId::ALL
        .into_iter()
        .find(|profile| profile.codex_name() == name)
}

fn has_parent_component(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
}

fn is_absolute_or_home_path(path: &str) -> bool {
    let path = path.trim();
    Path::new(path).is_absolute()
        || path == "~"
        || path.starts_with("~/")
        || path.starts_with("~\\")
        || path.starts_with("\\\\")
        || path.as_bytes().get(1) == Some(&b':')
}

fn deduplicate_strings(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileSystemSandboxEntry, NetworkDomainPermission, NetworkProxyMode};

    fn project_profile() -> CustomPermissionProfile {
        CustomPermissionProfile {
            description: Some("Project edit with secret carve-outs".to_string()),
            extends: Some(":workspace".to_string()),
            workspace_roots: BTreeMap::from([("~/code/shared".to_string(), true)]),
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::GlobPattern {
                        pattern: "**/*.env".to_string(),
                    },
                }]),
                glob_scan_max_depth: Some(4),
                ..Default::default()
            }),
            network: Some(NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Limited),
                domains: Some(BTreeMap::from([(
                    "api.example.com".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn resolves_custom_profile_extending_workspace() {
        let config = PermissionProfileConfiguration {
            profiles: BTreeMap::from([("project-edit".to_string(), project_profile())]),
            allowed_permission_profiles: Some(BTreeMap::from([
                (":read-only".to_string(), true),
                ("project-edit".to_string(), true),
            ])),
        };
        config.validate().expect("valid profile configuration");
        let resolved = config
            .resolve(
                "project-edit",
                vec!["/workspace".to_string()],
                Some("revision".to_string()),
                PermissionProfileProvenance::Managed,
            )
            .expect("resolve profile");

        assert_eq!(
            resolved.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(
            resolved.effective_permissions.active_profile.id,
            "project-edit"
        );
        assert_eq!(
            resolved
                .effective_permissions
                .active_profile
                .extends
                .as_deref(),
            Some(":workspace")
        );
        assert_eq!(
            resolved.effective_permissions.provenance,
            PermissionProfileProvenance::Managed
        );
        assert!(resolved
            .effective_permissions
            .runtime_workspace_roots
            .contains(&"~/code/shared".to_string()));
        let NetworkPermissionPolicy::Restricted { requirements } =
            resolved.effective_permissions.network
        else {
            panic!("network must stay restricted");
        };
        assert_eq!(requirements.enabled, Some(true));
        assert_eq!(requirements.mode, Some(NetworkProxyMode::Limited));
    }

    #[test]
    fn resolves_independent_minimal_profile_without_inheriting_root_read() {
        let config = PermissionProfileConfiguration {
            profiles: BTreeMap::from([(
                "minimal-project".to_string(),
                CustomPermissionProfile {
                    description: Some("Minimal runtime plus workspace".to_string()),
                    extends: None,
                    file_system: Some(AdditionalFileSystemPermissions {
                        entries: Some(vec![
                            FileSystemSandboxEntry {
                                access: FileSystemAccessMode::Read,
                                path: FileSystemPath::Special {
                                    value: FileSystemSpecialPath::Minimal,
                                },
                            },
                            FileSystemSandboxEntry {
                                access: FileSystemAccessMode::Write,
                                path: FileSystemPath::Special {
                                    value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                                },
                            },
                        ]),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };

        let resolved = config
            .resolve(
                "minimal-project",
                vec!["/workspace".to_string()],
                None,
                PermissionProfileProvenance::User,
            )
            .expect("resolve independent minimal profile");
        assert_eq!(resolved.effective_permissions.active_profile.extends, None);
        let FileSystemPermissionPolicy::Restricted { entries, .. } =
            resolved.effective_permissions.file_system
        else {
            panic!("minimal profile must stay restricted");
        };
        assert!(entries.iter().any(|entry| {
            entry.access == FileSystemAccessMode::Read
                && matches!(
                    entry.path,
                    FileSystemPath::Special {
                        value: FileSystemSpecialPath::Minimal
                    }
                )
        }));
        assert!(!entries.iter().any(|entry| {
            matches!(
                entry.path,
                FileSystemPath::Special {
                    value: FileSystemSpecialPath::Root
                }
            )
        }));
    }

    #[test]
    fn rejects_cycles_reserved_names_and_unsafe_bases() {
        let cycle = PermissionProfileConfiguration {
            profiles: BTreeMap::from([
                (
                    "a".to_string(),
                    CustomPermissionProfile {
                        extends: Some("b".to_string()),
                        ..Default::default()
                    },
                ),
                (
                    "b".to_string(),
                    CustomPermissionProfile {
                        extends: Some("a".to_string()),
                        ..Default::default()
                    },
                ),
            ]),
            ..Default::default()
        };
        assert!(cycle.validate().unwrap_err().contains("cycle"));

        let danger = PermissionProfileConfiguration {
            profiles: BTreeMap::from([(
                "unsafe".to_string(),
                CustomPermissionProfile {
                    extends: Some(":danger-full-access".to_string()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        assert!(danger.validate().is_err());

        let reserved = PermissionProfileConfiguration {
            profiles: BTreeMap::from([(
                "filesystem".to_string(),
                CustomPermissionProfile {
                    extends: Some(":read-only".to_string()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        assert!(reserved.validate().is_err());
    }

    #[test]
    fn allowlist_is_complete_for_custom_and_future_profiles() {
        let config = PermissionProfileConfiguration {
            profiles: BTreeMap::from([("project-edit".to_string(), project_profile())]),
            allowed_permission_profiles: Some(BTreeMap::from([("project-edit".to_string(), true)])),
        };
        assert!(config.profile_allowed("project-edit"));
        assert!(!config.profile_allowed(":workspace"));
        assert!(!config.profile_allowed("future-profile"));
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use toml::Value;

use crate::{
    AdditionalFileSystemPermissions, CustomPermissionProfile, FileSystemAccessMode, FileSystemPath,
    FileSystemSandboxEntry, FileSystemSpecialPath, NetworkDomainPermission, NetworkProxyMode,
    NetworkRequirements, PermissionProfileConfiguration, PermissionProfileProvenance,
};

/// Codex-compatible permission fields extracted from a larger `config.toml` document.
///
/// Unrelated top-level Codex settings are ignored so callers may parse a complete config file.
/// Fields inside permission profiles are strict: unsupported keys fail closed instead of being
/// silently dropped from the effective sandbox policy. A single parsed document may be a partial
/// configuration layer; callers must validate the final merged configuration before execution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexPermissionProfileDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_permissions: Option<String>,
    pub configuration: PermissionProfileConfiguration,
}

pub fn parse_codex_permission_profile_toml(
    input: &str,
) -> Result<CodexPermissionProfileDocument, String> {
    let root = toml::from_str::<Value>(input)
        .map_err(|err| format!("parse permission profile TOML failed: {err}"))?;
    let root = root
        .as_table()
        .ok_or_else(|| "permission profile TOML root must be a table".to_string())?;

    let default_permissions = root
        .get("default_permissions")
        .map(|value| required_string(value, "default_permissions"))
        .transpose()?;
    let profiles = root
        .get("permissions")
        .map(parse_profiles)
        .transpose()?
        .unwrap_or_default();
    let allowed_permission_profiles = root
        .get("allowed_permission_profiles")
        .map(parse_allowed_profiles)
        .transpose()?;
    let configuration = PermissionProfileConfiguration {
        profiles,
        allowed_permission_profiles,
    };
    Ok(CodexPermissionProfileDocument {
        default_permissions,
        configuration,
    })
}

/// Parse the dedicated managed-requirements policy format.
///
/// Unlike a complete Codex config file, a managed-requirements layer may only contain permission
/// profile fields. Rejecting unrelated top-level keys prevents an administrator typo from being
/// silently accepted as an empty policy.
pub fn parse_managed_requirements_toml(
    input: &str,
) -> Result<CodexPermissionProfileDocument, String> {
    let root = toml::from_str::<Value>(input)
        .map_err(|err| format!("parse managed requirements TOML failed: {err}"))?;
    let root = root
        .as_table()
        .ok_or_else(|| "managed requirements TOML root must be a table".to_string())?;
    for key in root.keys() {
        if !matches!(
            key.as_str(),
            "default_permissions" | "permissions" | "allowed_permission_profiles"
        ) {
            return Err(format!(
                "unsupported managed requirements top-level key: {key}"
            ));
        }
    }
    parse_codex_permission_profile_toml(input)
}

/// Merge a lower-precedence Codex config layer with a higher-precedence layer.
///
/// Scalar values use the higher layer when present. Profile, workspace-root, domain, socket, and
/// allowlist tables merge by key. Filesystem entries merge by normalized path, so a higher layer
/// can replace one path rule without discarding unrelated rules from lower layers.
pub fn merge_codex_permission_profile_documents(
    lower: CodexPermissionProfileDocument,
    higher: CodexPermissionProfileDocument,
) -> Result<CodexPermissionProfileDocument, String> {
    let document = merge_codex_permission_profile_document_layers(lower, higher);
    document.configuration.validate()?;
    if let Some(profile_name) = document.default_permissions.as_deref() {
        document.configuration.resolve(
            profile_name,
            Vec::new(),
            None,
            PermissionProfileProvenance::User,
        )?;
    }
    Ok(document)
}

/// Merge two partial Codex configuration layers without validating unresolved cross-layer
/// references. Callers must validate after all ordinary and managed layers have been composed.
pub fn merge_codex_permission_profile_document_layers(
    lower: CodexPermissionProfileDocument,
    higher: CodexPermissionProfileDocument,
) -> CodexPermissionProfileDocument {
    let mut profiles = lower.configuration.profiles;
    for (name, profile) in higher.configuration.profiles {
        profiles
            .entry(name)
            .and_modify(|lower| merge_custom_profile(lower, profile.clone()))
            .or_insert(profile);
    }
    let allowed_permission_profiles = merge_optional_map(
        lower.configuration.allowed_permission_profiles,
        higher.configuration.allowed_permission_profiles,
    );
    CodexPermissionProfileDocument {
        default_permissions: higher.default_permissions.or(lower.default_permissions),
        configuration: PermissionProfileConfiguration {
            profiles,
            allowed_permission_profiles,
        },
    }
}

fn merge_custom_profile(lower: &mut CustomPermissionProfile, higher: CustomPermissionProfile) {
    lower.description = higher.description.or(lower.description.take());
    lower.extends = higher.extends.or(lower.extends.take());
    lower.workspace_roots.extend(higher.workspace_roots);
    lower.file_system = merge_optional_file_system(lower.file_system.take(), higher.file_system);
    lower.network = merge_optional_network(lower.network.take(), higher.network);
}

fn merge_optional_file_system(
    lower: Option<AdditionalFileSystemPermissions>,
    higher: Option<AdditionalFileSystemPermissions>,
) -> Option<AdditionalFileSystemPermissions> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => {
            let mut entries = lower.normalized_entries();
            for entry in higher.normalized_entries() {
                entries.retain(|existing| existing.path != entry.path);
                entries.push(entry);
            }
            Some(AdditionalFileSystemPermissions {
                entries: Some(entries),
                glob_scan_max_depth: higher.glob_scan_max_depth.or(lower.glob_scan_max_depth),
                ..Default::default()
            })
        }
        (Some(lower), None) => Some(lower),
        (None, Some(higher)) => Some(higher),
        (None, None) => None,
    }
}

fn merge_optional_network(
    lower: Option<NetworkRequirements>,
    higher: Option<NetworkRequirements>,
) -> Option<NetworkRequirements> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => Some(NetworkRequirements {
            enabled: higher.enabled.or(lower.enabled),
            domains: merge_optional_map(lower.domains, higher.domains),
            unix_sockets: merge_optional_map(lower.unix_sockets, higher.unix_sockets),
            allow_local_binding: higher.allow_local_binding.or(lower.allow_local_binding),
            allow_upstream_proxy: higher.allow_upstream_proxy.or(lower.allow_upstream_proxy),
            mode: higher.mode.or(lower.mode),
            enable_socks5: higher.enable_socks5.or(lower.enable_socks5),
            enable_socks5_udp: higher.enable_socks5_udp.or(lower.enable_socks5_udp),
            dangerously_allow_all_unix_sockets: higher
                .dangerously_allow_all_unix_sockets
                .or(lower.dangerously_allow_all_unix_sockets),
            dangerously_allow_non_loopback_proxy: higher
                .dangerously_allow_non_loopback_proxy
                .or(lower.dangerously_allow_non_loopback_proxy),
            managed_allowed_domains_only: higher
                .managed_allowed_domains_only
                .or(lower.managed_allowed_domains_only),
            http_port: higher.http_port.or(lower.http_port),
            socks_port: higher.socks_port.or(lower.socks_port),
            allowed_domains: higher.allowed_domains.or(lower.allowed_domains),
            denied_domains: higher.denied_domains.or(lower.denied_domains),
            allow_unix_sockets: higher.allow_unix_sockets.or(lower.allow_unix_sockets),
        }),
        (Some(lower), None) => Some(lower),
        (None, Some(higher)) => Some(higher),
        (None, None) => None,
    }
}

fn merge_optional_map<K: Ord, V>(
    lower: Option<BTreeMap<K, V>>,
    higher: Option<BTreeMap<K, V>>,
) -> Option<BTreeMap<K, V>> {
    match (lower, higher) {
        (Some(mut lower), Some(higher)) => {
            lower.extend(higher);
            Some(lower)
        }
        (Some(lower), None) => Some(lower),
        (None, Some(higher)) => Some(higher),
        (None, None) => None,
    }
}

fn parse_profiles(value: &Value) -> Result<BTreeMap<String, CustomPermissionProfile>, String> {
    let profiles = required_table(value, "permissions")?;
    profiles
        .iter()
        .map(|(name, value)| parse_profile(name, value).map(|profile| (name.clone(), profile)))
        .collect()
}

fn parse_profile(name: &str, value: &Value) -> Result<CustomPermissionProfile, String> {
    let table = required_table(value, format!("permissions.{name}").as_str())?;
    reject_unknown_keys(
        table,
        &[
            "description",
            "extends",
            "workspace_roots",
            "filesystem",
            "network",
        ],
        format!("permissions.{name}").as_str(),
    )?;
    Ok(CustomPermissionProfile {
        description: table
            .get("description")
            .map(|value| required_string(value, "profile description"))
            .transpose()?,
        extends: table
            .get("extends")
            .map(|value| required_string(value, "profile extends"))
            .transpose()?,
        workspace_roots: table
            .get("workspace_roots")
            .map(|value| parse_boolean_map(value, "workspace_roots"))
            .transpose()?
            .unwrap_or_default(),
        file_system: table.get("filesystem").map(parse_filesystem).transpose()?,
        network: table.get("network").map(parse_network).transpose()?,
    })
}

fn parse_filesystem(value: &Value) -> Result<AdditionalFileSystemPermissions, String> {
    let table = required_table(value, "filesystem")?;
    let glob_scan_max_depth = table
        .get("glob_scan_max_depth")
        .map(|value| {
            let depth = value
                .as_integer()
                .ok_or_else(|| "glob_scan_max_depth must be an integer".to_string())?;
            usize::try_from(depth)
                .map_err(|_| "glob_scan_max_depth must be a positive integer".to_string())
        })
        .transpose()?;
    let mut entries = Vec::new();
    for (path, permission) in table {
        if path == "glob_scan_max_depth" {
            continue;
        }
        match permission {
            Value::String(access) => entries.push(FileSystemSandboxEntry {
                access: parse_access(access.as_str())?,
                path: direct_file_system_path(path)?,
            }),
            Value::Table(scoped) => {
                for (subpath, access) in scoped {
                    let access = required_string(access, "scoped filesystem access")?;
                    entries.push(FileSystemSandboxEntry {
                        access: parse_access(access.as_str())?,
                        path: scoped_file_system_path(path, subpath)?,
                    });
                }
            }
            _ => {
                return Err(format!(
                    "filesystem permission {path:?} must be read, write, deny, or a scoped table"
                ))
            }
        }
    }
    Ok(AdditionalFileSystemPermissions {
        entries: Some(entries),
        glob_scan_max_depth,
        ..Default::default()
    })
}

fn direct_file_system_path(path: &str) -> Result<FileSystemPath, String> {
    match path {
        ":root" => Ok(special_path(FileSystemSpecialPath::Root)),
        ":minimal" => Ok(special_path(FileSystemSpecialPath::Minimal)),
        ":workspace_roots" | ":project_roots" => {
            Ok(special_path(FileSystemSpecialPath::ProjectRoots {
                subpath: None,
            }))
        }
        ":tmpdir" => Ok(special_path(FileSystemSpecialPath::Tmpdir)),
        ":slash_tmp" => Ok(special_path(FileSystemSpecialPath::SlashTmp)),
        value if value.starts_with(':') => {
            Err(format!("unsupported special filesystem path {value:?}"))
        }
        value if contains_glob(value) => Ok(FileSystemPath::GlobPattern {
            pattern: value.to_string(),
        }),
        value => Ok(FileSystemPath::Path {
            path: value.to_string(),
        }),
    }
}

fn scoped_file_system_path(base: &str, subpath: &str) -> Result<FileSystemPath, String> {
    if has_parent_component(subpath) {
        return Err(format!(
            "scoped filesystem subpath must not contain parent traversal: {subpath:?}"
        ));
    }
    match base {
        ":workspace_roots" | ":project_roots" => {
            if subpath == "." {
                return Ok(special_path(FileSystemSpecialPath::ProjectRoots {
                    subpath: None,
                }));
            }
            if contains_glob(subpath) {
                return Ok(FileSystemPath::GlobPattern {
                    pattern: subpath.to_string(),
                });
            }
            Ok(special_path(FileSystemSpecialPath::ProjectRoots {
                subpath: Some(subpath.to_string()),
            }))
        }
        value if value.starts_with(':') => Err(format!(
            "special filesystem path {value:?} does not support scoped subpaths"
        )),
        value => {
            let combined = if subpath == "." {
                value.to_string()
            } else {
                format!("{}/{}", value.trim_end_matches(['/', '\\']), subpath)
            };
            if contains_glob(combined.as_str()) {
                Ok(FileSystemPath::GlobPattern { pattern: combined })
            } else {
                Ok(FileSystemPath::Path { path: combined })
            }
        }
    }
}

fn special_path(value: FileSystemSpecialPath) -> FileSystemPath {
    FileSystemPath::Special { value }
}

fn parse_access(value: &str) -> Result<FileSystemAccessMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" => Ok(FileSystemAccessMode::Read),
        "write" => Ok(FileSystemAccessMode::Write),
        "deny" | "none" => Ok(FileSystemAccessMode::Deny),
        other => Err(format!("unsupported filesystem access {other:?}")),
    }
}

fn parse_network(value: &Value) -> Result<NetworkRequirements, String> {
    let table = required_table(value, "network")?;
    reject_unknown_keys(
        table,
        &[
            "enabled",
            "domains",
            "unix_sockets",
            "allow_local_binding",
            "allow_upstream_proxy",
            "mode",
            "enable_socks5",
            "enable_socks5_udp",
            "dangerously_allow_all_unix_sockets",
            "dangerously_allow_non_loopback_proxy",
            "managed_allowed_domains_only",
            "http_port",
            "socks_port",
        ],
        "network",
    )?;
    Ok(NetworkRequirements {
        enabled: optional_bool(table, "enabled")?,
        domains: table
            .get("domains")
            .map(|value| parse_domain_map(value, "network.domains"))
            .transpose()?,
        unix_sockets: table
            .get("unix_sockets")
            .map(|value| parse_domain_map(value, "network.unix_sockets"))
            .transpose()?,
        allow_local_binding: optional_bool(table, "allow_local_binding")?,
        allow_upstream_proxy: optional_bool(table, "allow_upstream_proxy")?,
        mode: table
            .get("mode")
            .map(|value| {
                match required_string(value, "network.mode")?
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "limited" => Ok(NetworkProxyMode::Limited),
                    "full" => Ok(NetworkProxyMode::Full),
                    other => Err(format!("unsupported network proxy mode {other:?}")),
                }
            })
            .transpose()?,
        enable_socks5: optional_bool(table, "enable_socks5")?,
        enable_socks5_udp: optional_bool(table, "enable_socks5_udp")?,
        dangerously_allow_all_unix_sockets: optional_bool(
            table,
            "dangerously_allow_all_unix_sockets",
        )?,
        dangerously_allow_non_loopback_proxy: optional_bool(
            table,
            "dangerously_allow_non_loopback_proxy",
        )?,
        managed_allowed_domains_only: optional_bool(table, "managed_allowed_domains_only")?,
        http_port: optional_port(table, "http_port")?,
        socks_port: optional_port(table, "socks_port")?,
        ..NetworkRequirements::default()
    })
}

fn parse_domain_map(
    value: &Value,
    label: &str,
) -> Result<BTreeMap<String, NetworkDomainPermission>, String> {
    required_table(value, label)?
        .iter()
        .map(|(pattern, value)| {
            let permission = match required_string(value, label)?.to_ascii_lowercase().as_str() {
                "allow" => NetworkDomainPermission::Allow,
                "deny" => NetworkDomainPermission::Deny,
                other => return Err(format!("unsupported {label} permission {other:?}")),
            };
            Ok((pattern.clone(), permission))
        })
        .collect()
}

fn parse_allowed_profiles(value: &Value) -> Result<BTreeMap<String, bool>, String> {
    parse_boolean_map(value, "allowed_permission_profiles")
}

fn parse_boolean_map(value: &Value, label: &str) -> Result<BTreeMap<String, bool>, String> {
    required_table(value, label)?
        .iter()
        .map(|(key, value)| {
            value
                .as_bool()
                .map(|enabled| (key.clone(), enabled))
                .ok_or_else(|| format!("{label}.{key} must be a boolean"))
        })
        .collect()
}

fn optional_bool(table: &toml::map::Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    table
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("network.{key} must be a boolean"))
        })
        .transpose()
}

fn optional_port(table: &toml::map::Map<String, Value>, key: &str) -> Result<Option<u16>, String> {
    table
        .get(key)
        .map(|value| {
            let port = value
                .as_integer()
                .ok_or_else(|| format!("network.{key} must be an integer"))?;
            u16::try_from(port).map_err(|_| format!("network.{key} must be between 0 and 65535"))
        })
        .transpose()
}

fn required_string(value: &Value, label: &str) -> Result<String, String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{label} must be a string"))
}

fn required_table<'a>(
    value: &'a Value,
    label: &str,
) -> Result<&'a toml::map::Map<String, Value>, String> {
    value
        .as_table()
        .ok_or_else(|| format!("{label} must be a table"))
}

fn reject_unknown_keys(
    table: &toml::map::Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<(), String> {
    if let Some(key) = table.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("unsupported {label} key {key:?}"));
    }
    Ok(())
}

fn contains_glob(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, '*' | '?' | '['))
}

fn has_parent_component(value: &str) -> bool {
    Path::new(value)
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileSystemPermissionPolicy, PermissionProfileId};

    #[test]
    fn parses_codex_style_independent_minimal_profile() {
        let document = parse_codex_permission_profile_toml(
            r#"
model = "gpt-5"
default_permissions = "project-edit"

[allowed_permission_profiles]
":read-only" = true
"project-edit" = true

[permissions.project-edit]
description = "Minimal project editing"

[permissions.project-edit.workspace_roots]
"~/code/shared" = true

[permissions.project-edit.filesystem]
glob_scan_max_depth = 4
":minimal" = "read"

[permissions.project-edit.filesystem.":workspace_roots"]
"." = "write"
".devcontainer" = "read"
"**/*.env" = "deny"

[permissions.project-edit.network]
enabled = true
mode = "limited"

[permissions.project-edit.network.domains]
"api.example.com" = "allow"
"blocked.example.com" = "deny"
"#,
        )
        .expect("parse Codex permission TOML");

        assert_eq!(
            document.default_permissions.as_deref(),
            Some("project-edit")
        );
        let resolved = document
            .configuration
            .resolve(
                "project-edit",
                vec!["/workspace".to_string()],
                None,
                PermissionProfileProvenance::User,
            )
            .expect("resolve parsed profile");
        assert_eq!(
            resolved.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(resolved.effective_permissions.active_profile.extends, None);
        let FileSystemPermissionPolicy::Restricted { entries, .. } =
            resolved.effective_permissions.file_system
        else {
            panic!("filesystem must remain restricted");
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
    fn parses_parent_first_overrides_and_project_roots_alias() {
        let document = parse_codex_permission_profile_toml(
            r#"
default_permissions = "child"

[permissions.base.filesystem]
":root" = "read"
"/tmp/shared" = "read"

[permissions.base.filesystem.":project_roots"]
"docs" = "read"

[permissions.child]
extends = "base"

[permissions.child.filesystem]
"/tmp/shared" = "write"

[permissions.child.filesystem.":project_roots"]
"docs" = "write"
"#,
        )
        .expect("parse inherited profile");
        let resolved = document
            .configuration
            .resolve(
                "child",
                vec!["/workspace".to_string()],
                None,
                PermissionProfileProvenance::User,
            )
            .expect("resolve inherited profile");
        let FileSystemPermissionPolicy::Restricted { entries, .. } =
            resolved.effective_permissions.file_system
        else {
            panic!("restricted filesystem");
        };
        assert!(entries.iter().any(|entry| {
            entry.access == FileSystemAccessMode::Write
                && entry.path
                    == FileSystemPath::Path {
                        path: "/tmp/shared".to_string(),
                    }
        }));
    }

    #[test]
    fn rejects_unsupported_network_keys_and_parent_traversal() {
        let network_error = parse_codex_permission_profile_toml(
            r#"
[permissions.net.network]
enabled = true
proxy_url = "http://127.0.0.1:3128"
"#,
        )
        .expect_err("unsupported network keys must fail closed");
        assert!(network_error.contains("unsupported network key"));

        let traversal_error = parse_codex_permission_profile_toml(
            r#"
[permissions.escape.filesystem.":workspace_roots"]
"../outside" = "write"
"#,
        )
        .expect_err("parent traversal must fail closed");
        assert!(traversal_error.contains("parent traversal"));
    }

    #[test]
    fn higher_precedence_layers_override_by_key_without_dropping_other_rules() {
        let lower = parse_codex_permission_profile_toml(
            r#"
default_permissions = "dev"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"dev" = true

[permissions.dev]
description = "User profile"

[permissions.dev.filesystem]
":minimal" = "read"
"/tmp/shared" = "read"

[permissions.dev.filesystem.":workspace_roots"]
"." = "write"
"docs" = "read"

[permissions.dev.network]
enabled = true

[permissions.dev.network.domains]
"user.example.com" = "allow"
"shared.example.com" = "allow"
"#,
        )
        .expect("lower layer");
        let higher = parse_codex_permission_profile_toml(
            r#"
[allowed_permission_profiles]
":workspace" = false

[permissions.dev]
description = "Project profile"

[permissions.dev.filesystem]
"/tmp/shared" = "deny"

[permissions.dev.filesystem.":workspace_roots"]
"docs" = "write"

[permissions.dev.network.domains]
"project.example.com" = "allow"
"shared.example.com" = "deny"
"#,
        )
        .expect("higher layer");

        let merged = merge_codex_permission_profile_documents(lower, higher)
            .expect("merge permission profile layers");
        assert_eq!(merged.default_permissions.as_deref(), Some("dev"));
        assert_eq!(
            merged
                .configuration
                .allowed_permission_profiles
                .as_ref()
                .and_then(|allowed| allowed.get(":workspace")),
            Some(&false)
        );
        let profile = merged
            .configuration
            .profiles
            .get("dev")
            .expect("merged dev profile");
        assert_eq!(profile.description.as_deref(), Some("Project profile"));
        let entries = profile
            .file_system
            .as_ref()
            .expect("merged filesystem")
            .normalized_entries();
        assert!(entries.iter().any(|entry| {
            entry.access == FileSystemAccessMode::Read
                && matches!(
                    entry.path,
                    FileSystemPath::Special {
                        value: FileSystemSpecialPath::Minimal
                    }
                )
        }));
        assert!(entries.iter().any(|entry| {
            entry.access == FileSystemAccessMode::Deny
                && entry.path
                    == (FileSystemPath::Path {
                        path: "/tmp/shared".to_string(),
                    })
        }));
        let domains = profile
            .network
            .as_ref()
            .and_then(|network| network.domains.as_ref())
            .expect("merged domains");
        assert_eq!(
            domains.get("shared.example.com"),
            Some(&NetworkDomainPermission::Deny)
        );
        assert!(domains.contains_key("user.example.com"));
        assert!(domains.contains_key("project.example.com"));
    }

    #[test]
    fn managed_requirements_reject_unrelated_top_level_keys() {
        assert!(parse_codex_permission_profile_toml("model = \"gpt-test\"").is_ok());
        assert!(parse_managed_requirements_toml("model = \"gpt-test\"").is_err());
        assert!(parse_managed_requirements_toml("default_permissions = \":read-only\"").is_ok());
    }
}

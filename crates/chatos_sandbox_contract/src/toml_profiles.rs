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
mod merge;

use merge::*;

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
mod tests;

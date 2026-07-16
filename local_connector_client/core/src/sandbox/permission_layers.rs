// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::{
    merge_codex_permission_profile_document_layers, parse_codex_permission_profile_toml,
    parse_managed_requirements_toml, CodexPermissionProfileDocument, CustomPermissionProfile,
    PermissionProfileConfiguration, PermissionProfileId, PermissionProfileProvenance,
};

use crate::config::{home_dir, optional_env};
use sha2::{Digest, Sha256};

const MAX_PERMISSION_CONFIG_BYTES: u64 = 1024 * 1024;
const SYSTEM_CONFIG_ENV: &str = "LOCAL_CONNECTOR_SYSTEM_PERMISSIONS_CONFIG";
const USER_CONFIG_ENV: &str = "LOCAL_CONNECTOR_USER_PERMISSIONS_CONFIG";

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimePermissionProfileLayers {
    system: Option<CodexPermissionProfileDocument>,
    user: Option<CodexPermissionProfileDocument>,
    managed: Option<CodexPermissionProfileDocument>,
    load_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct EffectivePermissionProfileConfiguration {
    pub(crate) configuration: PermissionProfileConfiguration,
    pub(crate) default_profile_name: String,
    pub(crate) default_provenance: PermissionProfileProvenance,
    pub(crate) profile_provenance: BTreeMap<String, PermissionProfileProvenance>,
    pub(crate) managed_profile_names: BTreeSet<String>,
    managed_allowed_profile_names: BTreeSet<String>,
}

impl EffectivePermissionProfileConfiguration {
    pub(crate) fn provenance_for(&self, profile_name: &str) -> PermissionProfileProvenance {
        self.profile_provenance
            .get(profile_name)
            .copied()
            .unwrap_or(PermissionProfileProvenance::BuiltIn)
    }

    pub(crate) fn api_locked_profile_names(&self) -> BTreeSet<String> {
        let mut locked = self.managed_profile_names.clone();
        locked.extend(
            self.managed_allowed_profile_names
                .iter()
                .filter(|name| !name.starts_with(':'))
                .cloned(),
        );
        let mut pending = locked.iter().cloned().collect::<Vec<_>>();
        while let Some(profile_name) = pending.pop() {
            let Some(parent) = self
                .configuration
                .profiles
                .get(&profile_name)
                .and_then(|profile| profile.extends.as_ref())
                .filter(|parent| !parent.starts_with(':'))
            else {
                continue;
            };
            if locked.insert(parent.clone()) {
                pending.push(parent.clone());
            }
        }
        locked
    }
}

impl RuntimePermissionProfileLayers {
    pub(crate) fn blocked(message: impl Into<String>) -> Self {
        Self {
            load_error: Some(message.into()),
            ..Default::default()
        }
    }

    pub(crate) fn load_from_environment_with_cloud_managed(
        cloud_managed: Option<CodexPermissionProfileDocument>,
    ) -> Result<Self> {
        let system =
            permission_config_path(SYSTEM_CONFIG_ENV, default_system_permission_config_path());
        let user = permission_config_path(USER_CONFIG_ENV, default_user_permission_config_path());
        let managed = ConfigPath {
            path: default_managed_permission_config_path(),
            required: false,
            secure_system_file: true,
        };
        let mut layers = Self::load_from_paths(system, user, managed)?;
        layers.managed = merge_optional_documents(layers.managed, cloud_managed)?;
        Ok(layers)
    }

    fn load_from_paths(system: ConfigPath, user: ConfigPath, managed: ConfigPath) -> Result<Self> {
        Ok(Self {
            system: load_permission_document("system permission config", &system, false)?,
            user: load_permission_document("user permission config", &user, false)?,
            managed: load_permission_document("managed permission requirements", &managed, true)?,
            load_error: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn effective_configuration(
        &self,
        persisted_profiles: &BTreeMap<String, CustomPermissionProfile>,
        persisted_allowed_profiles: Option<&BTreeMap<String, bool>>,
        persisted_default_profile_name: Option<&str>,
        legacy_default_profile_id: PermissionProfileId,
    ) -> Result<EffectivePermissionProfileConfiguration> {
        self.effective_configuration_with_project(
            persisted_profiles,
            persisted_allowed_profiles,
            persisted_default_profile_name,
            legacy_default_profile_id,
            None,
        )
    }

    pub(crate) fn effective_configuration_with_project(
        &self,
        persisted_profiles: &BTreeMap<String, CustomPermissionProfile>,
        persisted_allowed_profiles: Option<&BTreeMap<String, bool>>,
        persisted_default_profile_name: Option<&str>,
        legacy_default_profile_id: PermissionProfileId,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> Result<EffectivePermissionProfileConfiguration> {
        if let Some(error) = self.load_error.as_deref() {
            return Err(anyhow!(
                "managed requirements are unresolved and sandbox permissions are blocked: {error}"
            ));
        }
        let configured_default_provenance = if project
            .and_then(|document| document.default_permissions.as_ref())
            .is_some()
        {
            PermissionProfileProvenance::Project
        } else if persisted_default_profile_name.is_some()
            || self
                .user
                .as_ref()
                .and_then(|document| document.default_permissions.as_ref())
                .is_some()
            || self
                .system
                .as_ref()
                .and_then(|document| document.default_permissions.as_ref())
                .is_some()
        {
            PermissionProfileProvenance::User
        } else {
            PermissionProfileProvenance::BuiltIn
        };
        let project_profile_names = project
            .into_iter()
            .flat_map(|document| document.configuration.profiles.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let persisted_document = (!persisted_profiles.is_empty()
            || persisted_allowed_profiles.is_some()
            || persisted_default_profile_name.is_some())
        .then(|| CodexPermissionProfileDocument {
            default_permissions: persisted_default_profile_name.map(ToOwned::to_owned),
            configuration: PermissionProfileConfiguration {
                profiles: persisted_profiles.clone(),
                allowed_permission_profiles: persisted_allowed_profiles.cloned(),
            },
        });
        let mut ordinary = merge_optional_documents(self.system.clone(), self.user.clone())?;
        ordinary = merge_optional_documents(ordinary, persisted_document)?;
        let pre_project_allowed_profiles = ordinary
            .as_ref()
            .and_then(|document| document.configuration.allowed_permission_profiles.clone());
        let project_allowed_profiles =
            project.and_then(|document| document.configuration.allowed_permission_profiles.clone());
        ordinary = merge_optional_documents(ordinary, project.cloned())?;
        let mut ordinary = ordinary.unwrap_or_default();
        ordinary.configuration.allowed_permission_profiles =
            intersect_complete_allowlists(pre_project_allowed_profiles, project_allowed_profiles);

        let mut profile_provenance = ordinary
            .configuration
            .profiles
            .keys()
            .map(|name| (name.clone(), PermissionProfileProvenance::User))
            .collect::<BTreeMap<_, _>>();
        for profile_name in project_profile_names {
            profile_provenance.insert(profile_name, PermissionProfileProvenance::Project);
        }
        let mut managed_profile_names = BTreeSet::new();
        let managed_allowed_profiles = self
            .managed
            .as_ref()
            .and_then(|document| document.configuration.allowed_permission_profiles.as_ref());
        let managed_allowed_profile_names = managed_allowed_profiles
            .into_iter()
            .flat_map(|allowed| allowed.iter())
            .filter(|(_, enabled)| **enabled)
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>();

        if let Some(managed) = self.managed.as_ref() {
            for (profile_name, profile) in &managed.configuration.profiles {
                if ordinary.configuration.profiles.contains_key(profile_name) {
                    return Err(anyhow!(
                        "managed permission profile {profile_name:?} conflicts with a config-defined profile of the same name"
                    ));
                }
                ordinary
                    .configuration
                    .profiles
                    .insert(profile_name.clone(), profile.clone());
                profile_provenance
                    .insert(profile_name.clone(), PermissionProfileProvenance::Managed);
                managed_profile_names.insert(profile_name.clone());
            }
        }

        validate_managed_requirements(self.managed.as_ref(), &ordinary.configuration.profiles)?;
        validate_ordinary_allowlist(
            ordinary.configuration.allowed_permission_profiles.as_ref(),
            &ordinary.configuration.profiles,
        )?;

        ordinary.configuration.allowed_permission_profiles = compose_allowed_profiles(
            ordinary.configuration.allowed_permission_profiles.as_ref(),
            managed_allowed_profiles,
            &ordinary.configuration.profiles,
        );
        ordinary
            .configuration
            .validate()
            .map_err(anyhow::Error::msg)?;

        let configured_default = ordinary
            .default_permissions
            .clone()
            .unwrap_or_else(|| legacy_default_profile_id.codex_name().to_string());
        let managed_fallback = managed_default_profile(self.managed.as_ref())?;
        let (default_profile_name, default_provenance) = select_default_profile(
            &ordinary.configuration,
            configured_default.as_str(),
            managed_fallback,
            &profile_provenance,
            configured_default_provenance,
        )?;

        Ok(EffectivePermissionProfileConfiguration {
            configuration: ordinary.configuration,
            default_profile_name,
            default_provenance,
            profile_provenance,
            managed_profile_names,
            managed_allowed_profile_names,
        })
    }

    #[cfg(test)]
    pub(crate) fn effective_policy_revision(
        &self,
        persisted_revision: Option<&str>,
    ) -> Option<String> {
        self.effective_policy_revision_with_project(persisted_revision, None)
    }

    pub(crate) fn effective_policy_revision_with_project(
        &self,
        persisted_revision: Option<&str>,
        project: Option<&CodexPermissionProfileDocument>,
    ) -> Option<String> {
        let runtime_revision = runtime_layers_revision(self, project)?;
        Some(match persisted_revision {
            Some(persisted) => format!("{persisted}+{runtime_revision}"),
            None => runtime_revision,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_tests(
        system: Option<CodexPermissionProfileDocument>,
        user: Option<CodexPermissionProfileDocument>,
        managed: Option<CodexPermissionProfileDocument>,
    ) -> Self {
        Self {
            system,
            user,
            managed,
            load_error: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ConfigPath {
    path: Option<PathBuf>,
    required: bool,
    secure_system_file: bool,
}

fn permission_config_path(environment_key: &str, default: Option<PathBuf>) -> ConfigPath {
    match optional_env(environment_key) {
        Some(path) => ConfigPath {
            path: Some(PathBuf::from(path)),
            required: true,
            secure_system_file: false,
        },
        None => ConfigPath {
            path: default,
            required: false,
            secure_system_file: false,
        },
    }
}

#[cfg(not(windows))]
fn default_system_permission_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/chatos/config.toml"))
}

#[cfg(windows)]
fn default_system_permission_config_path() -> Option<PathBuf> {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|path| {
            path.join("ChatOS")
                .join("LocalConnector")
                .join("config.toml")
        })
}

fn default_user_permission_config_path() -> Option<PathBuf> {
    home_dir().map(|path| {
        path.join(".chatos")
            .join("local_connector")
            .join("config.toml")
    })
}

#[cfg(not(windows))]
fn default_managed_permission_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/chatos/requirements.toml"))
}

#[cfg(windows)]
fn default_managed_permission_config_path() -> Option<PathBuf> {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|path| {
            path.join("ChatOS")
                .join("LocalConnector")
                .join("requirements.toml")
        })
}

fn load_permission_document(
    label: &str,
    configured_path: &ConfigPath,
    managed_requirements: bool,
) -> Result<Option<CodexPermissionProfileDocument>> {
    let Some(path) = configured_path.path.as_deref() else {
        return Ok(None);
    };
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound && !configured_path.required => {
            return Ok(None)
        }
        Err(err) => {
            return Err(err).with_context(|| format!("read {label} metadata {}", path.display()))
        }
    };
    if !metadata.is_file() {
        return Err(anyhow!("{label} {} is not a regular file", path.display()));
    }
    validate_secure_system_file(label, path, &metadata, configured_path.secure_system_file)?;
    if metadata.len() > MAX_PERMISSION_CONFIG_BYTES {
        return Err(anyhow!(
            "{label} {} exceeds the 1 MiB limit",
            path.display()
        ));
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("read {label} {}", path.display()))?;
    if source.len() as u64 > MAX_PERMISSION_CONFIG_BYTES {
        return Err(anyhow!(
            "{label} {} exceeds the 1 MiB limit",
            path.display()
        ));
    }
    let parsed = if managed_requirements {
        parse_managed_requirements_toml(source.as_str())
    } else {
        parse_codex_permission_profile_toml(source.as_str())
    };
    parsed
        .map(Some)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("parse {label} {}", path.display()))
}

#[cfg(unix)]
fn validate_secure_system_file(
    label: &str,
    path: &std::path::Path,
    metadata: &fs::Metadata,
    required: bool,
) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !required {
        return Ok(());
    }
    if metadata.uid() != 0 {
        return Err(anyhow!("{label} {} must be owned by root", path.display()));
    }
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(anyhow!(
            "{label} {} must not be group- or world-writable",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_secure_system_file(
    _label: &str,
    _path: &std::path::Path,
    _metadata: &fs::Metadata,
    _required: bool,
) -> Result<()> {
    Ok(())
}

fn merge_optional_documents(
    lower: Option<CodexPermissionProfileDocument>,
    higher: Option<CodexPermissionProfileDocument>,
) -> Result<Option<CodexPermissionProfileDocument>> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => Ok(Some(merge_codex_permission_profile_document_layers(
            lower, higher,
        ))),
        (Some(document), None) | (None, Some(document)) => Ok(Some(document)),
        (None, None) => Ok(None),
    }
}

fn validate_managed_requirements(
    managed: Option<&CodexPermissionProfileDocument>,
    available_profiles: &BTreeMap<String, CustomPermissionProfile>,
) -> Result<()> {
    let Some(managed) = managed else {
        return Ok(());
    };
    let Some(allowed) = managed.configuration.allowed_permission_profiles.as_ref() else {
        if managed.default_permissions.is_some() {
            return Err(anyhow!(
                "managed default_permissions requires allowed_permission_profiles"
            ));
        }
        return Ok(());
    };
    if allowed.is_empty() || !allowed.values().any(|enabled| *enabled) {
        return Err(anyhow!(
            "managed allowed_permission_profiles must enable at least one permission profile"
        ));
    }
    for profile_name in allowed.keys() {
        if builtin_profile_id(profile_name).is_none()
            && !available_profiles.contains_key(profile_name)
        {
            return Err(anyhow!(
                "managed allowed_permission_profiles refers to undefined profile {profile_name:?}"
            ));
        }
    }
    let Some(default_profile) = managed_default_profile(Some(managed))? else {
        return Ok(());
    };
    if !allowed.get(default_profile).copied().unwrap_or(false) {
        return Err(anyhow!(
            "managed default_permissions {default_profile:?} must be allowed by allowed_permission_profiles"
        ));
    }
    Ok(())
}

fn validate_ordinary_allowlist(
    allowed: Option<&BTreeMap<String, bool>>,
    available_profiles: &BTreeMap<String, CustomPermissionProfile>,
) -> Result<()> {
    let Some(allowed) = allowed else {
        return Ok(());
    };
    for profile_name in allowed.keys() {
        if builtin_profile_id(profile_name).is_none()
            && !available_profiles.contains_key(profile_name)
        {
            return Err(anyhow!(
                "allowed_permission_profiles refers to undefined profile {profile_name:?}"
            ));
        }
    }
    Ok(())
}

fn managed_default_profile(
    managed: Option<&CodexPermissionProfileDocument>,
) -> Result<Option<&str>> {
    let Some(managed) = managed else {
        return Ok(None);
    };
    let Some(allowed) = managed.configuration.allowed_permission_profiles.as_ref() else {
        return Ok(None);
    };
    if let Some(default_profile) = managed.default_permissions.as_deref() {
        return Ok(Some(default_profile));
    }
    let allows_workspace = allowed
        .get(PermissionProfileId::WorkspaceWrite.codex_name())
        .copied()
        .unwrap_or(false);
    let allows_read_only = allowed
        .get(PermissionProfileId::ReadOnly.codex_name())
        .copied()
        .unwrap_or(false);
    if allows_workspace && allows_read_only {
        Ok(Some(PermissionProfileId::WorkspaceWrite.codex_name()))
    } else {
        Err(anyhow!(
            "managed default_permissions must be set unless allowed_permission_profiles allows both :workspace and :read-only"
        ))
    }
}

fn compose_allowed_profiles(
    ordinary: Option<&BTreeMap<String, bool>>,
    managed: Option<&BTreeMap<String, bool>>,
    profiles: &BTreeMap<String, CustomPermissionProfile>,
) -> Option<BTreeMap<String, bool>> {
    if ordinary.is_none() && managed.is_none() {
        return None;
    }
    let mut names = PermissionProfileId::ALL
        .into_iter()
        .map(|profile| profile.codex_name().to_string())
        .collect::<BTreeSet<_>>();
    names.extend(profiles.keys().cloned());
    Some(
        names
            .into_iter()
            .map(|name| {
                let ordinary_allowed =
                    ordinary.is_none_or(|allowed| allowed.get(&name).copied().unwrap_or(false));
                let managed_allowed =
                    managed.is_none_or(|allowed| allowed.get(&name).copied().unwrap_or(false));
                (name, ordinary_allowed && managed_allowed)
            })
            .collect(),
    )
}

fn intersect_complete_allowlists(
    lower: Option<BTreeMap<String, bool>>,
    higher: Option<BTreeMap<String, bool>>,
) -> Option<BTreeMap<String, bool>> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => {
            let names = lower
                .keys()
                .chain(higher.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(
                names
                    .into_iter()
                    .map(|name| {
                        let lower_allowed = lower.get(&name).copied().unwrap_or(false);
                        let higher_allowed = higher.get(&name).copied().unwrap_or(false);
                        (name, lower_allowed && higher_allowed)
                    })
                    .collect(),
            )
        }
        (Some(allowed), None) | (None, Some(allowed)) => Some(allowed),
        (None, None) => None,
    }
}

fn select_default_profile(
    configuration: &PermissionProfileConfiguration,
    configured_default: &str,
    managed_fallback: Option<&str>,
    profile_provenance: &BTreeMap<String, PermissionProfileProvenance>,
    configured_default_provenance: PermissionProfileProvenance,
) -> Result<(String, PermissionProfileProvenance)> {
    if configuration.profile_allowed(configured_default)
        && configuration
            .resolve(
                configured_default,
                Vec::new(),
                None,
                configured_selection_provenance(
                    profile_provenance,
                    configured_default,
                    configured_default_provenance,
                ),
            )
            .is_ok()
    {
        return Ok((
            configured_default.to_string(),
            configured_selection_provenance(
                profile_provenance,
                configured_default,
                configured_default_provenance,
            ),
        ));
    }
    if let Some(managed_fallback) = managed_fallback {
        if configuration.profile_allowed(managed_fallback)
            && configuration
                .resolve(
                    managed_fallback,
                    Vec::new(),
                    None,
                    PermissionProfileProvenance::Managed,
                )
                .is_ok()
        {
            return Ok((
                managed_fallback.to_string(),
                managed_selection_provenance(profile_provenance, managed_fallback),
            ));
        }
    }

    let mut candidates = configuration
        .catalog()
        .into_iter()
        .filter(|entry| entry.allowed)
        .filter_map(|entry| {
            configuration
                .resolve(
                    entry.id.as_str(),
                    Vec::new(),
                    None,
                    provenance_for_profile(profile_provenance, entry.id.as_str()),
                )
                .ok()
                .map(|resolved| (resolved.permission_profile_id.rank(), entry.id))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    let Some((_, profile_name)) = candidates.into_iter().next() else {
        return Err(anyhow!(
            "effective allowed_permission_profiles does not leave a resolvable permission profile"
        ));
    };
    let provenance = if managed_fallback.is_some() {
        managed_selection_provenance(profile_provenance, profile_name.as_str())
    } else {
        provenance_for_profile(profile_provenance, profile_name.as_str())
    };
    Ok((profile_name, provenance))
}

fn configured_selection_provenance(
    provenance: &BTreeMap<String, PermissionProfileProvenance>,
    profile_name: &str,
    configured_default_provenance: PermissionProfileProvenance,
) -> PermissionProfileProvenance {
    provenance
        .get(profile_name)
        .copied()
        .unwrap_or(configured_default_provenance)
}

fn managed_selection_provenance(
    provenance: &BTreeMap<String, PermissionProfileProvenance>,
    profile_name: &str,
) -> PermissionProfileProvenance {
    provenance
        .get(profile_name)
        .copied()
        .unwrap_or(PermissionProfileProvenance::Managed)
}

fn provenance_for_profile(
    provenance: &BTreeMap<String, PermissionProfileProvenance>,
    profile_name: &str,
) -> PermissionProfileProvenance {
    provenance
        .get(profile_name)
        .copied()
        .unwrap_or(PermissionProfileProvenance::BuiltIn)
}

fn builtin_profile_id(name: &str) -> Option<PermissionProfileId> {
    PermissionProfileId::ALL
        .into_iter()
        .find(|profile| profile.codex_name() == name)
}

fn runtime_layers_revision(
    layers: &RuntimePermissionProfileLayers,
    project: Option<&CodexPermissionProfileDocument>,
) -> Option<String> {
    if layers.system.is_none()
        && layers.user.is_none()
        && layers.managed.is_none()
        && project.is_none()
    {
        return None;
    }
    let mut hasher = Sha256::new();
    for (label, document) in [
        ("system", layers.system.as_ref()),
        ("user", layers.user.as_ref()),
        ("managed", layers.managed.as_ref()),
        ("project", project),
    ] {
        hasher.update(label.as_bytes());
        hasher.update([0]);
        if let Some(document) = document {
            hasher.update(serde_json::to_vec(document).unwrap_or_default());
        }
        hasher.update([0xff]);
    }
    Some(format!("runtime-{}", hex::encode(hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn parse(source: &str) -> CodexPermissionProfileDocument {
        parse_codex_permission_profile_toml(source).expect("parse permission document")
    }

    fn test_config_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "chatos-permission-layers-{name}-{}",
            Uuid::new_v4()
        ))
    }

    #[test]
    fn managed_allowlist_falls_back_from_disallowed_user_default() {
        let layers = RuntimePermissionProfileLayers {
            user: Some(parse("default_permissions = \":danger-full-access\"")),
            managed: Some(parse(
                r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
            )),
            ..Default::default()
        };

        let effective = layers
            .effective_configuration(
                &BTreeMap::new(),
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
            )
            .expect("managed fallback");

        assert_eq!(effective.default_profile_name, ":read-only");
        assert_eq!(
            effective.default_provenance,
            PermissionProfileProvenance::Managed
        );
        assert!(!effective
            .configuration
            .profile_allowed(":danger-full-access"));
    }

    #[test]
    fn unresolved_managed_requirements_block_all_permission_resolution() {
        let layers = RuntimePermissionProfileLayers::blocked("cloud fetch failed");

        let error = layers
            .effective_configuration(
                &BTreeMap::new(),
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
            )
            .expect_err("unresolved managed requirements must fail closed");

        assert!(error.to_string().contains("permissions are blocked"));
    }

    #[test]
    fn managed_profile_name_collision_fails_closed() {
        let layers = RuntimePermissionProfileLayers {
            user: Some(parse(
                r#"
[permissions.acme]
extends = ":read-only"
"#,
            )),
            managed: Some(parse(
                r#"
default_permissions = "acme"

[allowed_permission_profiles]
acme = true

[permissions.acme]
extends = ":workspace"
"#,
            )),
            ..Default::default()
        };

        let error = layers
            .effective_configuration(
                &BTreeMap::new(),
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
            )
            .expect_err("same-name managed profile must fail");
        assert!(error.to_string().contains("conflicts"));
    }

    #[test]
    fn user_allowlist_can_only_narrow_managed_allowlist() {
        let layers = RuntimePermissionProfileLayers {
            managed: Some(parse(
                r#"
default_permissions = ":workspace"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
            )),
            ..Default::default()
        };
        let persisted_allowed = BTreeMap::from([
            (":read-only".to_string(), true),
            (":workspace".to_string(), false),
        ]);

        let effective = layers
            .effective_configuration(
                &BTreeMap::new(),
                Some(&persisted_allowed),
                None,
                PermissionProfileId::WorkspaceWrite,
            )
            .expect("narrow managed allowlist");

        assert_eq!(effective.default_profile_name, ":read-only");
        assert!(effective.configuration.profile_allowed(":read-only"));
        assert!(!effective.configuration.profile_allowed(":workspace"));
    }

    #[test]
    fn managed_allowlist_can_reference_a_persisted_user_profile() {
        let layers = RuntimePermissionProfileLayers {
            managed: Some(parse(
                r#"
default_permissions = "team-review"

[allowed_permission_profiles]
team-review = true
"#,
            )),
            ..Default::default()
        };
        let persisted_profiles = BTreeMap::from([(
            "team-review".to_string(),
            CustomPermissionProfile {
                extends: Some(":read-only".to_string()),
                ..Default::default()
            },
        )]);

        let effective = layers
            .effective_configuration(
                &persisted_profiles,
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
            )
            .expect("managed allowlist may reference loaded user profile");

        assert_eq!(effective.default_profile_name, "team-review");
        assert!(effective.configuration.profile_allowed("team-review"));
        assert_eq!(
            effective.default_provenance,
            PermissionProfileProvenance::User
        );
    }

    #[test]
    fn managed_default_without_allowlist_is_invalid() {
        let layers = RuntimePermissionProfileLayers {
            managed: Some(parse("default_permissions = \":read-only\"")),
            ..Default::default()
        };
        let error = layers
            .effective_configuration(
                &BTreeMap::new(),
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
            )
            .expect_err("managed default requires allowlist");
        assert!(error
            .to_string()
            .contains("requires allowed_permission_profiles"));
    }

    #[test]
    fn trusted_project_layer_overrides_user_default_with_project_provenance() {
        let layers = RuntimePermissionProfileLayers {
            user: Some(parse("default_permissions = \":workspace\"")),
            ..Default::default()
        };
        let project = parse(
            r#"
default_permissions = "project-review"

[permissions.project-review]
extends = ":read-only"
"#,
        );

        let effective = layers
            .effective_configuration_with_project(
                &BTreeMap::new(),
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
                Some(&project),
            )
            .expect("project configuration");

        assert_eq!(effective.default_profile_name, "project-review");
        assert_eq!(
            effective.default_provenance,
            PermissionProfileProvenance::Project
        );
        assert_eq!(
            effective.provenance_for("project-review"),
            PermissionProfileProvenance::Project
        );
    }

    #[test]
    fn trusted_project_layer_cannot_bypass_managed_allowlist() {
        let layers = RuntimePermissionProfileLayers {
            managed: Some(parse(
                r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
            )),
            ..Default::default()
        };
        let project = parse("default_permissions = \":danger-full-access\"");

        let effective = layers
            .effective_configuration_with_project(
                &BTreeMap::new(),
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
                Some(&project),
            )
            .expect("managed project configuration");

        assert_eq!(effective.default_profile_name, ":read-only");
        assert_eq!(
            effective.default_provenance,
            PermissionProfileProvenance::Managed
        );
        assert!(!effective
            .configuration
            .profile_allowed(":danger-full-access"));
    }

    #[test]
    fn trusted_project_layer_cannot_reenable_user_disabled_profile() {
        let layers = RuntimePermissionProfileLayers::default();
        let user_allowed = BTreeMap::from([
            (":read-only".to_string(), true),
            (":danger-full-access".to_string(), false),
        ]);
        let project = parse(
            r#"
default_permissions = ":danger-full-access"

[allowed_permission_profiles]
":read-only" = true
":danger-full-access" = true
"#,
        );

        let effective = layers
            .effective_configuration_with_project(
                &BTreeMap::new(),
                Some(&user_allowed),
                None,
                PermissionProfileId::WorkspaceWrite,
                Some(&project),
            )
            .expect("project allowlist is capped by user allowlist");

        assert_eq!(effective.default_profile_name, ":read-only");
        assert!(effective.configuration.profile_allowed(":read-only"));
        assert!(!effective
            .configuration
            .profile_allowed(":danger-full-access"));
    }

    #[test]
    fn runtime_layers_contribute_to_policy_revision() {
        let layers = RuntimePermissionProfileLayers {
            user: Some(parse("default_permissions = \":read-only\"")),
            ..Default::default()
        };

        let revision = layers
            .effective_policy_revision(Some("local-user-change"))
            .expect("runtime revision");

        assert!(revision.starts_with("local-user-change+runtime-"));
        assert_eq!(revision.len(), "local-user-change+runtime-".len() + 64);
    }

    #[test]
    fn configured_runtime_files_load_in_precedence_order() {
        let system_path = test_config_path("system.toml");
        let user_path = test_config_path("user.toml");
        let managed_path = test_config_path("requirements.toml");
        fs::write(
            &system_path,
            r#"
default_permissions = ":read-only"

[permissions.review]
extends = ":read-only"
"#,
        )
        .expect("write system config");
        fs::write(&user_path, "default_permissions = \":workspace\"").expect("write user config");
        fs::write(
            &managed_path,
            r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
review = true
"#,
        )
        .expect("write managed requirements");

        let layers = RuntimePermissionProfileLayers::load_from_paths(
            ConfigPath {
                path: Some(system_path.clone()),
                required: true,
                secure_system_file: false,
            },
            ConfigPath {
                path: Some(user_path.clone()),
                required: true,
                secure_system_file: false,
            },
            ConfigPath {
                path: Some(managed_path.clone()),
                required: true,
                secure_system_file: false,
            },
        )
        .expect("load configured runtime layers");
        let effective = layers
            .effective_configuration(
                &BTreeMap::new(),
                None,
                None,
                PermissionProfileId::WorkspaceWrite,
            )
            .expect("resolve configured runtime layers");

        assert_eq!(effective.default_profile_name, ":workspace");
        assert!(effective.configuration.profiles.contains_key("review"));
        assert!(effective.configuration.profile_allowed("review"));

        let _ = fs::remove_file(system_path);
        let _ = fs::remove_file(user_path);
        let _ = fs::remove_file(managed_path);
    }

    #[test]
    fn explicit_missing_or_malformed_runtime_file_fails_closed() {
        let missing = test_config_path("missing.toml");
        let error = RuntimePermissionProfileLayers::load_from_paths(
            ConfigPath {
                path: Some(missing),
                required: true,
                secure_system_file: false,
            },
            ConfigPath {
                path: None,
                required: false,
                secure_system_file: false,
            },
            ConfigPath {
                path: None,
                required: false,
                secure_system_file: false,
            },
        )
        .expect_err("explicit missing config must fail");
        assert!(error.to_string().contains("metadata"));

        let malformed = test_config_path("malformed.toml");
        fs::write(&malformed, "[permissions").expect("write malformed config");
        let error = RuntimePermissionProfileLayers::load_from_paths(
            ConfigPath {
                path: Some(malformed.clone()),
                required: true,
                secure_system_file: false,
            },
            ConfigPath {
                path: None,
                required: false,
                secure_system_file: false,
            },
            ConfigPath {
                path: None,
                required: false,
                secure_system_file: false,
            },
        )
        .expect_err("malformed config must fail");
        assert!(error.to_string().contains("parse system permission config"));
        let _ = fs::remove_file(malformed);
    }

    #[cfg(unix)]
    #[test]
    fn managed_system_file_rejects_insecure_policy_file() {
        let managed = test_config_path("user-owned-requirements.toml");
        fs::write(
            &managed,
            r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
"#,
        )
        .expect("write user-owned managed config");
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&managed, fs::Permissions::from_mode(0o666))
                .expect("make managed config insecure");
        }

        let error = load_permission_document(
            "managed permission requirements",
            &ConfigPath {
                path: Some(managed.clone()),
                required: true,
                secure_system_file: true,
            },
            true,
        )
        .expect_err("managed policy must be root-owned");

        assert!(
            error.to_string().contains("owned by root")
                || error.to_string().contains("group- or world-writable")
        );
        let _ = fs::remove_file(managed);
    }

    #[test]
    fn managed_runtime_file_rejects_unrelated_top_level_keys() {
        let managed = test_config_path("strict-requirements.toml");
        fs::write(&managed, "model = \"gpt-test\"").expect("write managed config");

        let error = load_permission_document(
            "managed permission requirements",
            &ConfigPath {
                path: Some(managed.clone()),
                required: true,
                secure_system_file: false,
            },
            true,
        )
        .expect_err("unrelated managed keys must fail closed");

        assert!(format!("{error:#}").contains("unsupported managed requirements top-level key"));
        let _ = fs::remove_file(managed);
    }
}

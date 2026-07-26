// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::plugins::{PluginCredentialScope, PluginCredentialVault};

const CREDENTIAL_PLACEHOLDER_PREFIX: &str = "${credential:";
const MAX_TEMPLATE_BYTES: usize = 8 * 1024;
const MAX_HTTP_HEADERS: usize = 64;
const MAX_STDIO_ENVIRONMENT_VARIABLES: usize = 64;
const SECRET_HANDLE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(super) struct PluginCredentialBindings {
    vault: PluginCredentialVault,
    owner_user_id: String,
    device_id: String,
    plugin_id: String,
    release_id: String,
    component_key: String,
    secret_names: BTreeSet<String>,
    snapshot_sha256: String,
}

impl fmt::Debug for PluginCredentialBindings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginCredentialBindings")
            .field("plugin_id", &self.plugin_id)
            .field("release_id", &self.release_id)
            .field("component_key", &self.component_key)
            .field("secret_count", &self.secret_names.len())
            .field("snapshot_sha256", &self.snapshot_sha256)
            .finish_non_exhaustive()
    }
}

impl PluginCredentialBindings {
    pub(super) fn prepare(
        vault: Option<PluginCredentialVault>,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
        secret_names: BTreeSet<String>,
    ) -> Result<Option<Self>> {
        if secret_names.is_empty() {
            return Ok(None);
        }
        let vault = vault.context("Plugin MCP credential template requires Credential Vault")?;
        let mut bindings = Self {
            vault,
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            plugin_id: plugin_id.to_string(),
            release_id: release_id.to_string(),
            component_key: component_key.to_string(),
            secret_names,
            snapshot_sha256: String::new(),
        };
        bindings.snapshot_sha256 = bindings.current_snapshot_sha256()?;
        Ok(Some(bindings))
    }

    pub(super) fn snapshot_sha256(&self) -> &str {
        self.snapshot_sha256.as_str()
    }

    pub(super) fn verify(&self) -> Result<()> {
        if self.current_snapshot_sha256()? != self.snapshot_sha256 {
            bail!("Plugin MCP credential snapshot changed after prepare");
        }
        Ok(())
    }

    fn resolve(&self, secret_name: &str) -> Result<String> {
        if !self.secret_names.contains(secret_name) {
            bail!("Plugin MCP credential was not published during prepare");
        }
        let scope = PluginCredentialScope::new(
            self.owner_user_id.clone(),
            self.device_id.clone(),
            self.plugin_id.clone(),
            self.release_id.clone(),
            self.component_key.clone(),
            secret_name.to_string(),
        )?;
        let handle = self.vault.issue_handle(&scope, SECRET_HANDLE_TTL)?;
        let resolved = self.vault.resolve_handle(handle.as_str(), &scope);
        let _ = self.vault.revoke_handle(handle.as_str());
        let secret = resolved?;
        std::str::from_utf8(secret.as_bytes())
            .context("Plugin MCP credential is not UTF-8")
            .map(str::to_string)
    }

    fn current_snapshot_sha256(&self) -> Result<String> {
        let metadata = self.vault.list(
            self.owner_user_id.as_str(),
            self.device_id.as_str(),
            self.plugin_id.as_str(),
            self.release_id.as_str(),
        )?;
        let by_name = metadata
            .into_iter()
            .filter(|record| record.component_key == self.component_key)
            .map(|record| (record.secret_name, record.updated_at))
            .collect::<BTreeMap<_, _>>();
        let mut payload = format!(
            "chatos.plugin.mcp.credentials.v1\n{}\n{}\n{}\n{}\n{}",
            self.owner_user_id, self.device_id, self.plugin_id, self.release_id, self.component_key,
        );
        for secret_name in &self.secret_names {
            let updated_at = by_name
                .get(secret_name)
                .with_context(|| format!("Plugin MCP credential is missing: {secret_name}"))?;
            payload.push('\n');
            payload.push_str(secret_name.as_str());
            payload.push(':');
            payload.push_str(updated_at.as_str());
        }
        Ok(hex::encode(Sha256::digest(payload.as_bytes())))
    }
}

#[derive(Debug, Clone)]
pub(super) struct PluginStdioEnvironmentTemplates {
    variables: BTreeMap<String, String>,
}

impl PluginStdioEnvironmentTemplates {
    pub(super) fn parse(environment: &BTreeMap<String, String>) -> Result<Self> {
        if environment.len() > MAX_STDIO_ENVIRONMENT_VARIABLES {
            bail!("Plugin stdio MCP environment exceeds the variable count limit");
        }
        let mut variables = BTreeMap::new();
        for (name, value) in environment {
            validate_environment_name(name)?;
            let template = PluginValueTemplate::parse(value)?;
            let secret_name = template
                .secret_name
                .filter(|_| template.prefix.is_empty() && template.suffix.is_empty())
                .context(
                    "Plugin stdio MCP environment values must be exact Credential Vault templates",
                )?;
            variables.insert(name.clone(), secret_name);
        }
        Ok(Self { variables })
    }

    pub(super) fn secret_names(&self) -> BTreeSet<String> {
        self.variables.values().cloned().collect()
    }

    pub(super) fn variable_names(&self) -> impl Iterator<Item = String> + '_ {
        self.variables.keys().cloned()
    }

    pub(super) fn resolve(
        &self,
        bindings: Option<&PluginCredentialBindings>,
    ) -> Result<ResolvedPluginValues> {
        if let Some(bindings) = bindings {
            bindings.verify()?;
        }
        let mut values = HashMap::new();
        for (name, secret_name) in &self.variables {
            let bindings =
                bindings.context("Plugin stdio MCP credential bindings are unavailable")?;
            let secret = bindings.resolve(secret_name)?;
            if secret.contains('\0') {
                bail!("resolved Plugin stdio MCP environment value contains NUL");
            }
            values.insert(name.clone(), secret);
        }
        Ok(ResolvedPluginValues(values))
    }
}

#[derive(Debug, Clone)]
pub(super) struct PluginHttpHeaderTemplates {
    templates: BTreeMap<String, PluginValueTemplate>,
}

impl PluginHttpHeaderTemplates {
    pub(super) fn parse(headers: &BTreeMap<String, String>) -> Result<Self> {
        if headers.len() > MAX_HTTP_HEADERS {
            bail!("Plugin HTTP MCP exceeds the header count limit");
        }
        let mut templates = BTreeMap::new();
        for (name, value) in headers {
            let normalized_name = normalize_header_name(name)?;
            let template = PluginValueTemplate::parse(value)?;
            if template.secret_name.is_none() && !allows_literal_header(normalized_name.as_str()) {
                bail!(
                    "Plugin HTTP MCP custom header must use a Credential Vault template: {normalized_name}"
                );
            }
            if templates
                .insert(normalized_name.clone(), template)
                .is_some()
            {
                bail!("Plugin HTTP MCP contains a duplicate header: {normalized_name}");
            }
        }
        Ok(Self { templates })
    }

    pub(super) fn secret_names(&self) -> BTreeSet<String> {
        self.templates
            .values()
            .filter_map(|template| template.secret_name.clone())
            .collect()
    }

    pub(super) fn contains(&self, header_name: &str) -> bool {
        self.templates.contains_key(header_name)
    }

    pub(super) fn resolve(
        &self,
        bindings: Option<&PluginCredentialBindings>,
    ) -> Result<ResolvedPluginValues> {
        if let Some(bindings) = bindings {
            bindings.verify()?;
        }
        let mut values = HashMap::new();
        for (name, template) in &self.templates {
            let value = match template.secret_name.as_deref() {
                Some(secret_name) => {
                    let bindings =
                        bindings.context("Plugin HTTP MCP credential bindings are unavailable")?;
                    let mut secret = bindings.resolve(secret_name)?;
                    let value = format!("{}{}{}", template.prefix, secret, template.suffix);
                    secret.zeroize();
                    value
                }
                None => template.prefix.clone(),
            };
            reqwest::header::HeaderValue::try_from(value.as_str())
                .context("resolved Plugin HTTP MCP header value is invalid")?;
            values.insert(name.clone(), value);
        }
        Ok(ResolvedPluginValues(values))
    }
}

#[derive(Clone)]
struct PluginValueTemplate {
    prefix: String,
    secret_name: Option<String>,
    suffix: String,
}

impl fmt::Debug for PluginValueTemplate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PluginValueTemplate([REDACTED])")
    }
}

impl PluginValueTemplate {
    fn parse(value: &str) -> Result<Self> {
        if value.is_empty()
            || value.len() > MAX_TEMPLATE_BYTES
            || value.chars().any(|character| character.is_control())
        {
            bail!("Plugin MCP value template is empty, oversized, or contains control characters");
        }
        let Some(start) = value.find(CREDENTIAL_PLACEHOLDER_PREFIX) else {
            return Ok(Self {
                prefix: value.to_string(),
                secret_name: None,
                suffix: String::new(),
            });
        };
        let name_start = start + CREDENTIAL_PLACEHOLDER_PREFIX.len();
        let remainder = &value[name_start..];
        let end = remainder
            .find('}')
            .context("Plugin MCP credential template is missing closing brace")?;
        let secret_name = &remainder[..end];
        let suffix = &remainder[end + 1..];
        if secret_name.is_empty()
            || suffix.contains(CREDENTIAL_PLACEHOLDER_PREFIX)
            || value[..start].contains(CREDENTIAL_PLACEHOLDER_PREFIX)
        {
            bail!("Plugin MCP credential template must contain exactly one valid placeholder");
        }
        PluginCredentialScope::new(
            "owner",
            "device",
            "plugin",
            "release",
            "component",
            secret_name,
        )?;
        Ok(Self {
            prefix: value[..start].to_string(),
            secret_name: Some(secret_name.to_string()),
            suffix: suffix.to_string(),
        })
    }
}

pub(super) struct ResolvedPluginValues(HashMap<String, String>);

impl ResolvedPluginValues {
    pub(super) fn as_map(&self) -> &HashMap<String, String> {
        &self.0
    }

    pub(super) fn insert(&mut self, name: String, value: String) {
        if let Some(mut previous) = self.0.insert(name, value) {
            previous.zeroize();
        }
    }

    pub(super) fn cloned_map(&self) -> HashMap<String, String> {
        self.0.clone()
    }
}

impl fmt::Debug for ResolvedPluginValues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedPluginValues")
            .field("value_count", &self.0.len())
            .finish_non_exhaustive()
    }
}

impl Drop for ResolvedPluginValues {
    fn drop(&mut self) {
        for value in self.0.values_mut() {
            value.zeroize();
        }
    }
}

fn normalize_header_name(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    reqwest::header::HeaderName::from_bytes(normalized.as_bytes())
        .context("Plugin HTTP MCP header name is invalid")?;
    if matches!(
        normalized.as_str(),
        "host" | "content-length" | "transfer-encoding" | "connection" | "proxy-authorization"
    ) {
        bail!("Plugin HTTP MCP header is controlled by the Host: {normalized}");
    }
    Ok(normalized)
}

fn validate_environment_name(value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if !valid {
        bail!("Plugin stdio MCP environment variable name is invalid");
    }
    let normalized = value.to_ascii_uppercase();
    if matches!(
        normalized.as_str(),
        "PATH"
            | "HOME"
            | "SHELL"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "COMSPEC"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "USERPROFILE"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "PROMPT_COMMAND"
    ) || normalized.starts_with("LD_")
        || normalized.starts_with("DYLD_")
        || normalized.starts_with("XDG_")
    {
        bail!("Plugin stdio MCP environment variable is controlled by the Host");
    }
    Ok(())
}

fn allows_literal_header(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "accept-language"
            | "content-type"
            | "mcp-protocol-version"
            | "user-agent"
            | "x-plugin-client"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{PluginHttpHeaderTemplates, PluginStdioEnvironmentTemplates};

    #[test]
    fn sensitive_and_custom_headers_require_vault_templates() {
        assert!(PluginHttpHeaderTemplates::parse(&BTreeMap::from([(
            "Authorization".to_string(),
            "Bearer static-secret".to_string(),
        )]))
        .is_err());
        assert!(PluginHttpHeaderTemplates::parse(&BTreeMap::from([(
            "X-Custom-Auth".to_string(),
            "static-secret".to_string(),
        )]))
        .is_err());
    }

    #[test]
    fn vault_templates_publish_only_valid_secret_names() {
        let templates = PluginHttpHeaderTemplates::parse(&BTreeMap::from([
            (
                "Authorization".to_string(),
                "Bearer ${credential:access_token}".to_string(),
            ),
            ("X-Plugin-Client".to_string(), "chatos".to_string()),
        ]))
        .expect("parse credential header templates");
        assert_eq!(
            templates.secret_names().into_iter().collect::<Vec<_>>(),
            vec!["access_token"]
        );
        assert!(PluginHttpHeaderTemplates::parse(&BTreeMap::from([(
            "Authorization".to_string(),
            "Bearer ${credential:../../escape}".to_string(),
        )]))
        .is_err());
    }

    #[test]
    fn stdio_environment_requires_exact_vault_templates_and_safe_names() {
        let templates = PluginStdioEnvironmentTemplates::parse(&BTreeMap::from([(
            "DEMO_TOKEN".to_string(),
            "${credential:access_token}".to_string(),
        )]))
        .expect("parse stdio environment templates");
        assert_eq!(
            templates.secret_names().into_iter().collect::<Vec<_>>(),
            vec!["access_token"]
        );
        assert!(PluginStdioEnvironmentTemplates::parse(&BTreeMap::from([(
            "DEMO_TOKEN".to_string(),
            "Bearer ${credential:access_token}".to_string(),
        )]))
        .is_err());
        assert!(PluginStdioEnvironmentTemplates::parse(&BTreeMap::from([(
            "PATH".to_string(),
            "${credential:access_token}".to_string(),
        )]))
        .is_err());
    }
}

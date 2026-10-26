use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Display, sync::Arc};

const SERVICE: &str = "com.chatos.windows.plugin-credentials";
const MAXIMUM_SECRET_BYTES: usize = 2_560;

pub(crate) trait PluginCredentialStore: Send + Sync {
    fn load(&self, reference: &str) -> Result<Option<String>, String>;
    fn save(&self, reference: &str, value: &str) -> Result<(), String>;
    fn delete(&self, reference: &str) -> Result<(), String>;
}

pub(crate) fn platform_plugin_credential_store() -> Arc<dyn PluginCredentialStore> {
    Arc::new(PlatformPluginCredentialStore)
}

struct PlatformPluginCredentialStore;

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl PluginCredentialStore for PlatformPluginCredentialStore {
    fn load(&self, reference: &str) -> Result<Option<String>, String> {
        match credential_entry(reference)?.get_password() {
            Ok(value) if value.is_empty() => Ok(None),
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(safe_credential_error(error)),
        }
    }

    fn save(&self, reference: &str, value: &str) -> Result<(), String> {
        validate_secret(value)?;
        credential_entry(reference)?
            .set_password(value)
            .map_err(safe_credential_error)
    }

    fn delete(&self, reference: &str) -> Result<(), String> {
        match credential_entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(safe_credential_error(error)),
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn credential_entry(reference: &str) -> Result<keyring::Entry, String> {
    validate_reference(reference)?;
    keyring::Entry::new(SERVICE, reference).map_err(safe_credential_error)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl PluginCredentialStore for PlatformPluginCredentialStore {
    fn load(&self, _reference: &str) -> Result<Option<String>, String> {
        Err("当前平台没有可用的 Plugin 系统凭据存储。".into())
    }

    fn save(&self, _reference: &str, _value: &str) -> Result<(), String> {
        Err("当前平台没有可用的 Plugin 系统凭据存储。".into())
    }

    fn delete(&self, _reference: &str) -> Result<(), String> {
        Err("当前平台没有可用的 Plugin 系统凭据存储。".into())
    }
}

pub(crate) fn secret_reference(
    owner_user_id: &str,
    plugin_id: &str,
    secret_name: &str,
) -> Result<String, String> {
    validate_secret_name(secret_name)?;
    let mut digest = Sha256::new();
    digest.update(owner_user_id.as_bytes());
    digest.update([0]);
    digest.update(plugin_id.as_bytes());
    digest.update([0]);
    digest.update(secret_name.as_bytes());
    Ok(format!("plugin-secret-{:x}", digest.finalize()))
}

pub(crate) fn secret_name(template: &str) -> Result<&str, String> {
    template
        .strip_prefix("${credential:")
        .and_then(|value| value.strip_suffix('}'))
        .ok_or_else(invalid_secret_reference)
        .and_then(|value| {
            validate_secret_name(value)?;
            Ok(value)
        })
}

pub(crate) fn environment_secret_references(
    owner_user_id: &str,
    plugin_id: &str,
    environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    environment
        .iter()
        .map(|(variable, template)| {
            Ok((
                variable.clone(),
                secret_reference(owner_user_id, plugin_id, secret_name(template)?)?,
            ))
        })
        .collect()
}

pub(crate) fn validate_secret(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAXIMUM_SECRET_BYTES || value.contains('\0') {
        return Err("Plugin 凭据必须为 1 至 2560 字节，且不能包含 NUL。".into());
    }
    Ok(())
}

fn validate_secret_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(invalid_secret_reference());
    }
    Ok(())
}

fn validate_reference(reference: &str) -> Result<(), String> {
    if reference.len() != 78
        || !reference.starts_with("plugin-secret-")
        || !reference[14..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid_secret_reference());
    }
    Ok(())
}

fn invalid_secret_reference() -> String {
    "Plugin 凭据引用无效。".into()
}

fn safe_credential_error(error: impl Display) -> String {
    let _ = error;
    "无法访问系统 Plugin 凭据存储。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_scoped_without_exposing_identifiers() {
        let first = secret_reference("owner-1", "plugin-1", "token").unwrap();
        assert_ne!(
            first,
            secret_reference("owner-2", "plugin-1", "token").unwrap()
        );
        assert_ne!(
            first,
            secret_reference("owner-1", "plugin-2", "token").unwrap()
        );
        assert_ne!(
            first,
            secret_reference("owner-1", "plugin-1", "other").unwrap()
        );
        assert!(!first.contains("owner-1"));
        assert!(!first.contains("plugin-1"));
        assert!(!first.contains("token"));
    }

    #[test]
    fn environment_projection_rejects_non_credential_templates() {
        let environment = BTreeMap::from([("TOKEN".into(), "literal".into())]);
        assert!(environment_secret_references("owner", "plugin", &environment).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_credential_manager_round_trips_plugin_secret() {
        let store = PlatformPluginCredentialStore;
        let reference = secret_reference(
            &format!("test-owner-{}", uuid::Uuid::new_v4()),
            "test-plugin",
            "token",
        )
        .unwrap();
        store.save(&reference, "native-round-trip").unwrap();
        assert_eq!(
            store.load(&reference).unwrap().as_deref(),
            Some("native-round-trip")
        );
        store.delete(&reference).unwrap();
        assert_eq!(store.load(&reference).unwrap(), None);
    }
}

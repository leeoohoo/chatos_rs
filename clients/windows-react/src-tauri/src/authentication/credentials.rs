use sha2::{Digest, Sha256};
use std::fmt::Display;

pub trait CredentialStore: Send + Sync {
    fn load_access_token(&self) -> Result<Option<String>, String>;
    fn save_access_token(&self, token: &str) -> Result<(), String>;
    fn delete_access_token(&self) -> Result<(), String>;
    fn load_model_credential(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
    ) -> Result<Option<String>, String>;
    fn save_model_credential(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        credential: &str,
    ) -> Result<(), String>;
}

pub struct PlatformCredentialStore;

const SERVICE: &str = "com.chatos.windows.authentication";
const ACCOUNT: &str = "access-token";
const MODEL_SERVICE: &str = "com.chatos.windows.model-credentials";

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl CredentialStore for PlatformCredentialStore {
    fn load_access_token(&self) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(safe_credential_error)?;
        match entry.get_password() {
            Ok(token) => Ok(normalized_token(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(safe_credential_error(error)),
        }
    }

    fn save_access_token(&self, token: &str) -> Result<(), String> {
        let token = token.trim();
        if token.is_empty() {
            return self.delete_access_token();
        }
        keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(safe_credential_error)?
            .set_password(token)
            .map_err(safe_credential_error)
    }

    fn delete_access_token(&self) -> Result<(), String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(safe_credential_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(safe_credential_error(error)),
        }
    }

    fn load_model_credential(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
    ) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(
            MODEL_SERVICE,
            &model_credential_account(owner_user_id, model_config_ref),
        )
        .map_err(safe_credential_error)?;
        match entry.get_password() {
            Ok(credential) => Ok(normalized_token(credential)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(safe_credential_error(error)),
        }
    }

    fn save_model_credential(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        credential: &str,
    ) -> Result<(), String> {
        let credential = credential.trim();
        if credential.is_empty() {
            return Err("模型凭据为空。".into());
        }
        keyring::Entry::new(
            MODEL_SERVICE,
            &model_credential_account(owner_user_id, model_config_ref),
        )
        .map_err(safe_credential_error)?
        .set_password(credential)
        .map_err(safe_credential_error)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl CredentialStore for PlatformCredentialStore {
    fn load_access_token(&self) -> Result<Option<String>, String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn save_access_token(&self, _token: &str) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn delete_access_token(&self) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn load_model_credential(
        &self,
        _owner_user_id: &str,
        _model_config_ref: &str,
    ) -> Result<Option<String>, String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn save_model_credential(
        &self,
        _owner_user_id: &str,
        _model_config_ref: &str,
        _credential: &str,
    ) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }
}

pub(crate) fn model_credential_environment_variable(model_config_ref: &str) -> String {
    let normalized: String = model_config_ref
        .chars()
        .map(|value| {
            let value = value.to_ascii_uppercase();
            if value.is_ascii_uppercase() || value.is_ascii_digit() {
                value
            } else {
                '_'
            }
        })
        .take(96)
        .collect();
    format!("CHATOS_LOCAL_AGENT_MODEL_{normalized}")
}

fn model_credential_account(owner_user_id: &str, model_config_ref: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(owner_user_id.as_bytes());
    digest.update([0]);
    digest.update(model_config_ref.as_bytes());
    format!("model-{:x}", digest.finalize())
}

fn normalized_token(token: String) -> Option<String> {
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

fn safe_credential_error(error: impl Display) -> String {
    let _ = error;
    "无法访问系统凭据存储。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_environment_variable_matches_macos_normalization() {
        assert_eq!(
            model_credential_environment_variable("provider/model-1"),
            "CHATOS_LOCAL_AGENT_MODEL_PROVIDER_MODEL_1"
        );
    }

    #[test]
    fn model_credential_account_is_owner_scoped_without_leaking_identifiers() {
        let first = model_credential_account("owner-1", "model-1");
        let second = model_credential_account("owner-2", "model-1");
        assert_ne!(first, second);
        assert!(!first.contains("owner-1"));
        assert!(!first.contains("model-1"));
    }
}

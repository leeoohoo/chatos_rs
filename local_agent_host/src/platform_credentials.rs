// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;

use zeroize::Zeroizing;

pub const LOCAL_AGENT_CREDENTIAL_SERVICE: &str = "com.chatos.local-agent.credentials.v1";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalAgentPlatformCredentialError {
    #[error("local Agent credential identity is invalid")]
    InvalidIdentity,
    #[error("local Agent credential is unavailable")]
    Unavailable,
}

pub trait LocalAgentPlatformCredentialReader: Send + Sync {
    fn read(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError>;
}

pub fn platform_credential_account_key(
    owner_user_id: &str,
    reference: &str,
) -> Result<String, LocalAgentPlatformCredentialError> {
    validate_identity(owner_user_id)?;
    validate_identity(reference)?;
    Ok(format!(
        "v1:{}:{owner_user_id}{reference}",
        owner_user_id.len()
    ))
}

fn validate_identity(value: &str) -> Result<(), LocalAgentPlatformCredentialError> {
    if value.is_empty()
        || value.len() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(LocalAgentPlatformCredentialError::InvalidIdentity)
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub struct MacOsLocalAgentCredentialReader {
    service: String,
}

#[cfg(target_os = "macos")]
impl MacOsLocalAgentCredentialReader {
    pub fn production() -> Self {
        Self {
            service: LOCAL_AGENT_CREDENTIAL_SERVICE.to_string(),
        }
    }

    #[cfg(test)]
    fn for_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

#[cfg(target_os = "macos")]
impl fmt::Debug for MacOsLocalAgentCredentialReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacOsLocalAgentCredentialReader")
            .field("service", &"[KEYCHAIN SERVICE]")
            .finish()
    }
}

#[cfg(target_os = "macos")]
impl Default for MacOsLocalAgentCredentialReader {
    fn default() -> Self {
        Self::production()
    }
}

#[cfg(target_os = "macos")]
impl LocalAgentPlatformCredentialReader for MacOsLocalAgentCredentialReader {
    fn read(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        let account = platform_credential_account_key(owner_user_id, reference)?;
        let secret = security_framework::passwords::get_generic_password(&self.service, &account)
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        if secret.is_empty() {
            Err(LocalAgentPlatformCredentialError::Unavailable)
        } else {
            Ok(Zeroizing::new(secret))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_key_is_utf8_length_prefixed_and_collision_safe() {
        assert_eq!(
            platform_credential_account_key("用户-1", "postgres-1").unwrap(),
            "v1:8:用户-1postgres-1"
        );
        assert_ne!(
            platform_credential_account_key("account", "-secret").unwrap(),
            platform_credential_account_key("account-", "secret").unwrap()
        );
    }

    #[test]
    fn account_key_rejects_ambiguous_or_control_bearing_values() {
        for (owner, reference) in [
            ("", "secret"),
            (" user", "secret"),
            ("user", "secret\n"),
            ("user", ""),
        ] {
            assert_eq!(
                platform_credential_account_key(owner, reference).unwrap_err(),
                LocalAgentPlatformCredentialError::InvalidIdentity
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reader_uses_the_native_keychain_namespace() {
        use security_framework::passwords::{delete_generic_password, set_generic_password};

        let service = format!("com.chatos.local-agent.test.{}", std::process::id());
        let owner = "keychain-contract-user";
        let reference = "keychain-contract-secret";
        let account = platform_credential_account_key(owner, reference).unwrap();
        let _ = delete_generic_password(&service, &account);
        set_generic_password(&service, &account, b"temporary-contract-secret").unwrap();
        let reader = MacOsLocalAgentCredentialReader::for_service(service.clone());
        let secret = reader.read(owner, reference).unwrap();
        assert_eq!(secret.as_slice(), b"temporary-contract-secret");
        drop(secret);
        delete_generic_password(&service, &account).unwrap();
        assert_eq!(
            reader.read(owner, reference).unwrap_err(),
            LocalAgentPlatformCredentialError::Unavailable
        );
        assert!(!format!("{reader:?}").contains(service.as_str()));
    }
}

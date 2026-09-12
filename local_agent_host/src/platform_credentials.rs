// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::HashMap, fmt};
use zeroize::Zeroizing;

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

/// Resolves device-bound binary keys from the current launch's provisioned,
/// zeroizing credential set.
pub trait LocalAgentPlatformDeviceKeyReader: Send + Sync {
    fn read_device_key(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError>;
}

/// One-launch credential set provisioned by the native client over the
/// inherited anonymous bootstrap pipe. The client remains the sole owner of
/// Keychain/Credential Manager persistence; the Host keeps only zeroizing
/// in-memory values and resolves them by the frozen opaque references.
pub struct ProvidedLocalAgentCredentials {
    owner_user_id: String,
    values: HashMap<String, Zeroizing<Vec<u8>>>,
}

impl ProvidedLocalAgentCredentials {
    pub(crate) fn new(
        owner_user_id: String,
        values: HashMap<String, Zeroizing<Vec<u8>>>,
    ) -> Result<Self, LocalAgentPlatformCredentialError> {
        validate_identity(owner_user_id.as_str())?;
        if values.is_empty()
            || values
                .iter()
                .any(|(reference, value)| validate_identity(reference).is_err() || value.is_empty())
        {
            return Err(LocalAgentPlatformCredentialError::InvalidIdentity);
        }
        Ok(Self {
            owner_user_id,
            values,
        })
    }

    fn read_value(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        validate_identity(owner_user_id)?;
        validate_identity(reference)?;
        if owner_user_id != self.owner_user_id {
            return Err(LocalAgentPlatformCredentialError::Unavailable);
        }
        self.values
            .get(reference)
            .map(|value| Zeroizing::new(value.to_vec()))
            .ok_or(LocalAgentPlatformCredentialError::Unavailable)
    }
}

impl fmt::Debug for ProvidedLocalAgentCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProvidedLocalAgentCredentials")
            .field("owner_user_id", &"[OWNER]")
            .field("value_count", &self.values.len())
            .finish()
    }
}

impl LocalAgentPlatformCredentialReader for ProvidedLocalAgentCredentials {
    fn read(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        self.read_value(owner_user_id, reference)
    }
}

impl LocalAgentPlatformDeviceKeyReader for ProvidedLocalAgentCredentials {
    fn read_device_key(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        self.read_value(owner_user_id, reference)
    }
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

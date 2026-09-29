// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::LocalModelCredentialResolver;
use async_trait::async_trait;

/// Resolves credentials injected into the Local Agent Host child process by
/// the native client. The persisted reference must use `env:VARIABLE_NAME`;
/// neither the variable name nor its value is accepted on the command line.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChildEnvironmentModelCredentialResolver;

#[async_trait]
impl LocalModelCredentialResolver for ChildEnvironmentModelCredentialResolver {
    async fn resolve_model_api_key(
        &self,
        _owner_user_id: &str,
        credential_ref: &str,
    ) -> Result<String, String> {
        let variable = environment_variable_name(credential_ref)?;
        std::env::var(variable).map_err(|error| {
            format!("model credential environment variable {variable} is unavailable: {error}")
        })
    }
}

fn environment_variable_name(credential_ref: &str) -> Result<&str, String> {
    let variable = credential_ref
        .strip_prefix("env:")
        .ok_or_else(|| "standalone model credential_ref must use env:VARIABLE_NAME".to_string())?;
    if variable.is_empty()
        || variable.len() > 128
        || !variable
            .bytes()
            .all(|value| value == b'_' || value.is_ascii_alphanumeric())
    {
        return Err(
            "model credential environment variable must be 1..=128 ASCII letters, digits, or underscores"
                .to_string(),
        );
    }
    Ok(variable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_explicit_environment_references() {
        assert_eq!(
            environment_variable_name("env:CHATOS_MODEL_KEY"),
            Ok("CHATOS_MODEL_KEY")
        );
        assert!(environment_variable_name("keychain:model/default").is_err());
        assert!(environment_variable_name("env:BAD-NAME").is_err());
        assert!(environment_variable_name("env:").is_err());
    }
}

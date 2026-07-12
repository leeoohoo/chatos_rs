// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::env_config::env_text;

const MIN_PRODUCTION_SECRET_LENGTH: usize = 16;

pub fn is_production_environment() -> bool {
    let environment = env_text("CHATOS_ENV")
        .or_else(|| env_text("NODE_ENV"))
        .unwrap_or_else(|| "local".to_string());
    is_production_environment_value(environment.as_str())
}

pub fn validate_production_secret(
    name: &str,
    value: Option<&str>,
    insecure_values: &[&str],
) -> Result<(), String> {
    if !is_production_environment() {
        return Ok(());
    }
    validate_secret(name, value, insecure_values)
}

fn is_production_environment_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "production" | "prod"
    )
}

fn validate_secret(
    name: &str,
    value: Option<&str>,
    insecure_values: &[&str],
) -> Result<(), String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    let Some(value) = value else {
        return Err(format!("{name} must be configured in production"));
    };
    if value.len() < MIN_PRODUCTION_SECRET_LENGTH {
        return Err(format!(
            "{name} must contain at least {MIN_PRODUCTION_SECRET_LENGTH} characters in production"
        ));
    }
    if insecure_values.contains(&value) {
        return Err(format!(
            "{name} uses a known development default and must be changed in production"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_production_environment_value, validate_secret};

    #[test]
    fn recognizes_production_aliases() {
        assert!(is_production_environment_value("production"));
        assert!(is_production_environment_value(" PROD "));
        assert!(!is_production_environment_value("development"));
        assert!(!is_production_environment_value("local"));
    }

    #[test]
    fn rejects_missing_short_and_known_default_secrets() {
        assert!(validate_secret("TOKEN", None, &[]).is_err());
        assert!(validate_secret("TOKEN", Some("short"), &[]).is_err());
        assert!(validate_secret(
            "TOKEN",
            Some("known-development-default"),
            &["known-development-default"],
        )
        .is_err());
    }

    #[test]
    fn accepts_a_strong_non_default_secret() {
        validate_secret(
            "TOKEN",
            Some("this-is-a-long-unique-production-secret"),
            &["known-development-default"],
        )
        .expect("strong production secret");
    }
}

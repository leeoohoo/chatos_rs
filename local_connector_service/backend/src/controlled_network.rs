// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) fn normalize_windows_sid(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.len() > 184
        || !value.starts_with("S-1-")
        || value
            .split('-')
            .skip(1)
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("controlled-network Windows user SID is invalid".to_string());
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_sid_validation_is_strict() {
        assert_eq!(
            normalize_windows_sid(" S-1-5-21-100-200-300-400 ").unwrap(),
            "S-1-5-21-100-200-300-400"
        );
        for invalid in ["", "s-1-5-21", "S-1-", "S-1-5-name", "S-1-5--21"] {
            assert!(normalize_windows_sid(invalid).is_err(), "{invalid}");
        }
    }
}

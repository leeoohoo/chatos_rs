// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn normalized_identity_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub fn normalize_owned_identity_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_owned_identity_text, normalized_identity_text};

    #[test]
    fn identity_text_is_trimmed_and_empty_values_are_removed() {
        assert_eq!(normalized_identity_text(Some(" user ")), Some("user"));
        assert_eq!(normalized_identity_text(Some("  ")), None);
        assert_eq!(normalized_identity_text(None), None);
        assert_eq!(
            normalize_owned_identity_text(" agent ".to_string()),
            Some("agent".to_string())
        );
        assert_eq!(normalize_owned_identity_text(" ".to_string()), None);
    }
}

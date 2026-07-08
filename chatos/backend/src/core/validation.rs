// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn normalize_non_empty(input: Option<String>) -> Option<String> {
    input.and_then(|v| normalize_non_empty_str(&v))
}

pub fn normalize_non_empty_str(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_optional_string() {
        assert_eq!(
            normalize_non_empty(Some("  hello  ".to_string())),
            Some("hello".to_string())
        );
        assert_eq!(normalize_non_empty(Some("   ".to_string())), None);
        assert_eq!(normalize_non_empty(None), None);
    }

    #[test]
    fn normalizes_raw_string() {
        assert_eq!(
            normalize_non_empty_str("\n test \t"),
            Some("test".to_string())
        );
        assert_eq!(normalize_non_empty_str(""), None);
    }
}

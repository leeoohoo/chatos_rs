use std::path::Path;

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

pub fn validate_existing_dir(
    path: &str,
    empty_error: &str,
    invalid_error: &str,
) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(empty_error.to_string());
    }
    if !Path::new(trimmed).is_dir() {
        return Err(invalid_error.to_string());
    }
    Ok(trimmed.to_string())
}

pub fn validate_existing_dir_if_present(
    path: Option<String>,
    invalid_error: &str,
) -> Result<Option<String>, String> {
    match path {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if !Path::new(&trimmed).is_dir() {
                return Err(invalid_error.to_string());
            }
            Ok(Some(trimmed))
        }
        None => Ok(None),
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

    #[test]
    fn rejects_missing_directory() {
        let err = validate_existing_dir("", "empty", "invalid").unwrap_err();
        assert_eq!(err, "empty");

        let err =
            validate_existing_dir("/definitely-not-existing-path", "empty", "invalid").unwrap_err();
        assert_eq!(err, "invalid");
    }

    #[test]
    fn rejects_invalid_optional_directory() {
        let err = validate_existing_dir_if_present(Some("/not-existing".to_string()), "invalid")
            .unwrap_err();
        assert_eq!(err, "invalid");
    }
}

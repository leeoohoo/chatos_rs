use serde_json::{Map, Value};
use unicode_segmentation::UnicodeSegmentation;

const MAXIMUM_PURPOSE_CHARACTERS: usize = 500;

pub(crate) fn resolve_terminal_command(arguments: &Map<String, Value>) -> Option<&str> {
    ["command", "common"].into_iter().find_map(|key| {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

pub(crate) fn resolve_terminal_purpose(arguments: &Map<String, Value>) -> Option<&str> {
    arguments
        .get("purpose")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty() && value.graphemes(true).count() <= MAXIMUM_PURPOSE_CHARACTERS
        })
}

pub(crate) fn resolve_terminal_path(arguments: &Map<String, Value>) -> &str {
    arguments.get("path").and_then(Value::as_str).unwrap_or(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_prefers_canonical_field_and_preserves_legacy_fallback() {
        let canonical = json!({"command": " git status --short ", "common": "legacy"});
        assert_eq!(
            resolve_terminal_command(canonical.as_object().unwrap()),
            Some("git status --short")
        );
        let fallback = json!({"command": " \n", "common": " pwd "});
        assert_eq!(
            resolve_terminal_command(fallback.as_object().unwrap()),
            Some("pwd")
        );
    }

    #[test]
    fn purpose_is_trimmed_and_bounded_like_macos() {
        let valid = json!({"purpose": "  验证项目类型检查  "});
        assert_eq!(
            resolve_terminal_purpose(valid.as_object().unwrap()),
            Some("验证项目类型检查")
        );
        let emoji = "👨‍👩‍👧‍👦".repeat(MAXIMUM_PURPOSE_CHARACTERS);
        let grapheme_bounded = json!({"purpose": emoji});
        assert!(resolve_terminal_purpose(grapheme_bounded.as_object().unwrap()).is_some());
        let too_long = json!({"purpose": "a".repeat(MAXIMUM_PURPOSE_CHARACTERS + 1)});
        assert!(resolve_terminal_purpose(too_long.as_object().unwrap()).is_none());
    }
}

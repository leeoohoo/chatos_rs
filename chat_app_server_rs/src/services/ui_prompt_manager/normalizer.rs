#[path = "normalizer/choice.rs"]
mod choice;
#[path = "normalizer/fields.rs"]
mod fields;
#[path = "normalizer/redaction.rs"]
mod redaction;
#[path = "normalizer/submission.rs"]
mod submission;
#[path = "normalizer/support.rs"]
mod support;
#[path = "normalizer/values.rs"]
mod values;

use super::types::{UiPromptPayload, UiPromptResponseSubmission};

pub(super) use self::redaction::redact_prompt_payload;
pub use self::redaction::redact_response_for_store;
pub(super) use self::support::trimmed_non_empty;

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn normalize_kv_fields_derives_missing_keys_and_dedupes() {
        let input = json!([
            {
                "name": "repo",
                "label": "Repository"
            },
            {
                "label": "API Token"
            },
            {
                "key": "repo",
                "label": "Repository Mirror"
            }
        ]);

        let fields =
            super::fields::normalize_kv_fields(Some(&input), 50).expect("normalize fields");
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].key, "repo");
        assert_eq!(fields[1].key, "api_token");
        assert_eq!(fields[2].key, "repo_2");
    }
}

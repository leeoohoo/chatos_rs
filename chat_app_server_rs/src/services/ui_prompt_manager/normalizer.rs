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

pub use self::choice::{
    ChoiceLimits, ChoiceOption, LimitMode, normalize_choice_limits, normalize_choice_options,
    normalize_choice_selection, normalize_default_selection,
};
pub use self::fields::{KvField, normalize_kv_fields};
pub use self::redaction::{redact_prompt_payload, redact_response_for_store};
pub use self::submission::parse_response_submission;
pub(super) use self::support::trimmed_non_empty;
pub use self::values::normalize_kv_values;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::normalize_kv_fields;

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

        let fields = normalize_kv_fields(Some(&input), 50).expect("normalize fields");
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].key, "repo");
        assert_eq!(fields[1].key, "api_token");
        assert_eq!(fields[2].key, "repo_2");
    }
}

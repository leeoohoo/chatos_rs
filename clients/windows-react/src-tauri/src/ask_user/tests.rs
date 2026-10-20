use super::*;
use models::{AskUserChoice, AskUserChoiceOption, AskUserField};
use std::collections::BTreeMap;

fn sample_prompt() -> AskUserPrompt {
    AskUserPrompt {
        id: "local-ask:run-1".into(),
        session_id: "conversation-1".into(),
        turn_id: "turn-1".into(),
        tool_call_id: Some("call-1".into()),
        kind: "mixed".into(),
        status: AskUserPromptStatus::Pending,
        title: "Question".into(),
        message: "Continue?".into(),
        allows_cancel: true,
        timeout_milliseconds: None,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        fields: vec![AskUserField {
            key: "name".into(),
            label: "Name".into(),
            description: None,
            placeholder: None,
            default_value: String::new(),
            is_required: true,
            is_multiline: false,
            is_secret: false,
        }],
        choice: Some(AskUserChoice {
            allows_multiple: true,
            options: vec![AskUserChoiceOption {
                value: "a".into(),
                label: "A".into(),
                description: None,
            }],
            default_selection: Vec::new(),
            minimum_selection_count: 1,
            maximum_selection_count: 1,
        }),
    }
}

#[test]
fn validates_required_fields_and_choice_bounds() {
    let mut values = BTreeMap::new();
    values.insert("name".into(), "  ".into());
    let invalid = AskUserSubmissionInput {
        values,
        selection: Some(AskUserSelectionInput::Multiple(vec!["a".into()])),
    };
    assert!(validate_submission(&invalid, &sample_prompt()).is_err());
    let mut values = BTreeMap::new();
    values.insert("name".into(), "Ada".into());
    let valid = AskUserSubmissionInput {
        values,
        selection: Some(AskUserSelectionInput::Multiple(vec!["a".into()])),
    };
    assert!(validate_submission(&valid, &sample_prompt()).is_ok());
}

#[test]
fn rejects_nonempty_sensitive_values_before_ipc() {
    let mut prompt = sample_prompt();
    prompt.fields = vec![AskUserField {
        key: "api_token".into(),
        label: "Token".into(),
        description: None,
        placeholder: None,
        default_value: String::new(),
        is_required: false,
        is_multiline: false,
        is_secret: true,
    }];
    prompt.choice = None;
    let submission = AskUserSubmissionInput {
        values: BTreeMap::from([("api_token".into(), "secret".into())]),
        selection: None,
    };
    assert!(validate_submission(&submission, &prompt)
        .unwrap_err()
        .contains("Secret"));
}

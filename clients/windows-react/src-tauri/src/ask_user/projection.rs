use chatos_local_agent_protocol::LocalAgentEventRecord;
use serde_json::Value;

use super::models::{
    AskUserChoice, AskUserChoiceOption, AskUserField, AskUserPrompt, AskUserPromptStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskUserRunContext {
    pub conversation_id: String,
    pub turn_id: String,
}

pub fn run_context(profile_key: &str, input: &Value) -> Option<AskUserRunContext> {
    let object = input.as_object()?;
    let conversation_key = if profile_key == "main_chat" {
        "conversation_id"
    } else {
        "source_conversation_id"
    };
    let turn_key = if profile_key == "main_chat" {
        "turn_id"
    } else {
        "source_turn_id"
    };
    Some(AskUserRunContext {
        conversation_id: nonempty_string(object.get(conversation_key))?,
        turn_id: nonempty_string(object.get(turn_key))?,
    })
}

pub fn prompt(
    run_id: &str,
    updated_at_unix_ms: i64,
    context: &AskUserRunContext,
    event: &LocalAgentEventRecord,
) -> Option<AskUserPrompt> {
    let stored_value = event.payload.as_object()?.get("prompt")?;
    let stored = stored_value.as_object();
    let payload_value = stored
        .and_then(|object| object.get("payload"))
        .unwrap_or(stored_value);
    let payload = payload_value.as_object();
    let fields = payload
        .and_then(|object| object.get("fields"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter_map(|(index, value)| field(value, index))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let choice = payload
        .and_then(|object| object.get("choice"))
        .and_then(choice);
    let inferred_kind = match (fields.is_empty(), choice.is_some()) {
        (false, true) => "mixed",
        (false, false) => "fields",
        (true, true) => "choice",
        (true, false) => "confirmation",
    };
    let message = first_string(&[
        stored.and_then(|value| value.get("message")),
        stored.and_then(|value| value.get("question")),
        payload.and_then(|value| value.get("message")),
        payload.and_then(|value| value.get("question")),
        Some(stored_value),
    ])
    .unwrap_or_else(|| "请提供继续执行所需的信息。".to_owned());
    Some(AskUserPrompt {
        id: format!("local-ask:{run_id}"),
        session_id: context.conversation_id.clone(),
        turn_id: context.turn_id.clone(),
        tool_call_id: stored
            .and_then(|value| nonempty_string(value.get("tool_call_id")))
            .or_else(|| Some(run_id.to_owned())),
        kind: stored
            .and_then(|value| nonempty_string(value.get("kind")))
            .unwrap_or_else(|| inferred_kind.to_owned()),
        status: AskUserPromptStatus::Pending,
        title: stored
            .and_then(|value| nonempty_string(value.get("title")))
            .unwrap_or_else(|| "需要你的回复".to_owned()),
        message,
        allows_cancel: stored
            .and_then(|value| value.get("allow_cancel"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_milliseconds: stored
            .and_then(|value| value.get("timeout_ms"))
            .and_then(Value::as_i64),
        fields,
        choice,
        created_at_unix_ms: event.created_at_unix_ms,
        updated_at_unix_ms,
    })
}

pub fn sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    [
        "apikey",
        "accesstoken",
        "password",
        "passwd",
        "secret",
        "credential",
    ]
    .iter()
    .any(|candidate| normalized.contains(candidate))
}

fn field(value: &Value, index: usize) -> Option<AskUserField> {
    let object = value.as_object()?;
    let label = nonempty_string(object.get("label"));
    let key = first_string(&[object.get("key"), object.get("name"), object.get("id")])
        .unwrap_or_else(|| field_key(label.as_deref().unwrap_or(&format!("field_{}", index + 1))));
    if key.is_empty() {
        return None;
    }
    let explicit_secret = object
        .get("secret")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_secret = explicit_secret || sensitive_key(&key);
    Some(AskUserField {
        key: key.clone(),
        label: label.unwrap_or(key),
        description: nonempty_string(object.get("description")),
        placeholder: nonempty_string(object.get("placeholder")),
        default_value: if is_secret {
            String::new()
        } else {
            first_string(&[object.get("default_value"), object.get("default")]).unwrap_or_default()
        },
        is_required: object
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_multiline: object
            .get("multiline")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_secret,
    })
}

fn choice(value: &Value) -> Option<AskUserChoice> {
    let object = value.as_object()?;
    let options = object
        .get("options")?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let option = item.as_object()?;
            let value = nonempty_string(option.get("value"))?;
            Some(AskUserChoiceOption {
                label: nonempty_string(option.get("label")).unwrap_or_else(|| value.clone()),
                value,
                description: nonempty_string(option.get("description")),
            })
        })
        .collect::<Vec<_>>();
    if options.is_empty() {
        return None;
    }
    let allows_multiple = object
        .get("multiple")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let minimum_selection_count = integer(object.get("min_selections")).unwrap_or(0);
    let default_maximum = if allows_multiple { options.len() } else { 1 };
    let maximum_selection_count = integer(object.get("max_selections"))
        .unwrap_or(default_maximum)
        .max(minimum_selection_count);
    let default_selection = match object.get("default") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
        Some(Value::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    };
    Some(AskUserChoice {
        allows_multiple,
        options,
        default_selection,
        minimum_selection_count,
        maximum_selection_count,
    })
}

fn first_string(values: &[Option<&Value>]) -> Option<String> {
    values.iter().find_map(|value| nonempty_string(*value))
}

fn nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn integer(value: Option<&Value>) -> Option<usize> {
    value.and_then(Value::as_u64).map(|value| value as usize)
}

fn field_key(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_prompt_without_exposing_secret_defaults() {
        let event = LocalAgentEventRecord {
            cursor: 9,
            event_id: "event-1".into(),
            run_id: "run-1".into(),
            event_type: "user_input_requested".into(),
            payload: json!({"prompt": {"payload": {"fields": [
                {"label": "API Key", "default": "must-not-cross-ipc", "required": true}
            ], "choice": {"multiple": true, "min_selections": 1, "max_selections": 2,
                "options": [{"value": "a"}, {"value": "b"}]}}}}),
            created_at_unix_ms: 100,
        };
        let projected = prompt(
            "run-1",
            200,
            &AskUserRunContext {
                conversation_id: "conversation-1".into(),
                turn_id: "turn-1".into(),
            },
            &event,
        )
        .unwrap();
        assert_eq!(projected.id, "local-ask:run-1");
        assert!(projected.fields[0].is_secret);
        assert!(projected.fields[0].default_value.is_empty());
        assert_eq!(projected.choice.unwrap().maximum_selection_count, 2);
    }

    #[test]
    fn extracts_main_and_task_conversation_contexts() {
        assert_eq!(
            run_context(
                "main_chat",
                &json!({"conversation_id": "c", "turn_id": "t"})
            )
            .unwrap()
            .conversation_id,
            "c"
        );
        assert_eq!(
            run_context(
                "task_execution",
                &json!({"source_conversation_id": "c2", "source_turn_id": "t2"})
            )
            .unwrap()
            .turn_id,
            "t2"
        );
    }
}

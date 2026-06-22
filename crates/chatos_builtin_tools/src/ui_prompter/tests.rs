use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::*;

#[derive(Debug, Clone)]
struct EchoPromptStore;

#[async_trait]
impl UiPrompterStore for EchoPromptStore {
    async fn execute_prompt(
        &self,
        payload: UiPromptPayload,
        _on_stream_chunk: Option<UiPromptStreamChunkCallback>,
    ) -> Result<UiPromptDecision, String> {
        Ok(UiPromptDecision {
            status: "ok".to_string(),
            response: UiPromptResponseSubmission {
                status: "ok".to_string(),
                values: Some(payload.payload),
                selection: Some(Value::String("yes".to_string())),
                reason: None,
            },
        })
    }
}

fn test_service() -> UiPrompterService {
    UiPrompterService::new(UiPrompterOptions {
        server_name: "ui_prompter".to_string(),
        prompt_timeout_ms: 120_000,
        store: UiPrompterStoreRef::new(Arc::new(EchoPromptStore)),
    })
    .expect("ui prompter should initialize")
}

#[test]
fn ui_prompter_lists_expected_tools() {
    let tools = test_service().list_tools();
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(names.contains(&"prompt_key_values"));
    assert!(names.contains(&"prompt_choices"));
    assert!(names.contains(&"prompt_mixed_form"));
}

#[test]
fn mixed_form_requires_fields_or_choice() {
    let err = test_service()
        .call_tool(
            "prompt_mixed_form",
            json!({ "title": "Empty" }),
            Some("session_1"),
            Some("turn_1"),
            None,
        )
        .expect_err("mixed form should reject empty payload");
    assert!(err.contains("fields and/or choice"));
}

#[test]
fn key_value_prompt_normalizes_fields() {
    let result = test_service()
        .call_tool(
            "prompt_key_values",
            json!({
                "fields": [
                    { "label": "API Token", "secret": true },
                    { "name": "repo", "default": "main" }
                ]
            }),
            Some("session_1"),
            Some("turn_1"),
            None,
        )
        .expect("prompt should execute");
    let structured = result
        .get("_structured_result")
        .and_then(|value| value.get("values"))
        .expect("structured values");
    let fields = structured
        .get("fields")
        .and_then(Value::as_array)
        .expect("fields payload");
    assert_eq!(
        fields[0].get("key").and_then(Value::as_str),
        Some("api_token")
    );
    assert_eq!(fields[1].get("key").and_then(Value::as_str), Some("repo"));
}

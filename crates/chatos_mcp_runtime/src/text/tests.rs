// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use serde_json::json;

#[test]
fn transient_images_are_validated_and_removed_from_structured_history() {
    let payload = json!({
        "text": "screenshot captured",
        "_structured_result": {"persisted": false, "sha256": "abc"},
        "_model_input": [{
            "type": "input_image",
            "image_url": "data:image/jpeg;base64,/9j/AA==",
            "detail": "high"
        }]
    });
    let (text, mut structured) = to_text_and_structured_result_with_transient(&payload);
    assert_eq!(text, "screenshot captured");
    let transient = take_transient_model_input(&mut structured).expect("transient image");
    assert_eq!(transient.items().len(), 1);
    assert!(!structured
        .expect("structured result")
        .to_string()
        .contains("base64"));
}

#[test]
fn standard_mcp_image_content_becomes_transient_model_input() {
    let payload = json!({
        "content": [
            {"type": "text", "text": "App=md.obsidian"},
            {
                "type": "image",
                "data": "iVBORw0KGgo=",
                "mimeType": "image/png"
            }
        ],
        "isError": false
    });

    let (text, mut structured) = to_text_and_structured_result_with_transient(&payload);
    assert_eq!(text, "App=md.obsidian");
    let transient = take_transient_model_input(&mut structured).expect("transient image");
    assert_eq!(
        transient.items()[0]["image_url"],
        "data:image/png;base64,iVBORw0KGgo="
    );
    assert!(!structured
        .unwrap_or(Value::Null)
        .to_string()
        .contains("iVBORw0KGgo="));
}

#[test]
fn wrapped_mcp_result_exposes_text_structured_content_and_transient_image() {
    let payload = json!({
        "content": {
            "content": [
                {"type": "text", "text": "Active application: ChatOS"},
                {"type": "image", "data": "/9j/AA==", "mimeType": "image/jpeg"}
            ],
            "isError": false,
            "structuredContent": {
                "activeApplication": {"name": "ChatOS"}
            }
        },
        "is_error": false,
        "made_progress": true
    });

    let (text, mut structured) = to_text_and_structured_result_with_transient(&payload);
    let transient = take_transient_model_input(&mut structured).expect("transient image");

    assert_eq!(text, "Active application: ChatOS");
    assert_eq!(
        structured
            .as_ref()
            .and_then(|value| value.pointer("/activeApplication/name"))
            .and_then(Value::as_str),
        Some("ChatOS")
    );
    assert_eq!(
        transient.items()[0]["image_url"],
        "data:image/jpeg;base64,/9j/AA=="
    );
}

#[test]
fn remote_and_oversized_model_images_are_not_forwarded() {
    for image_url in [
        "https://example.com/screenshot.png".to_string(),
        format!(
            "data:image/png;base64,{}",
            STANDARD.encode(vec![0_u8; MODEL_INPUT_MAX_DECODED_BYTES + 1])
        ),
    ] {
        let payload = json!({
            "text": "capture",
            "_model_input": [{"image_url": image_url}]
        });
        let (_, mut structured) = to_text_and_structured_result_with_transient(&payload);
        assert!(take_transient_model_input(&mut structured).is_none());
    }
}

#[test]
fn explicit_text_limit_overrides_the_legacy_environment_default() {
    let payload = json!({
        "content": [{"type": "text", "text": "x".repeat(20_000)}]
    });

    let (text, _) = to_text_and_structured_result_with_transient_limit(&payload, 40_000);

    assert_eq!(text.chars().count(), 20_000);
    assert!(!text.contains("...[truncated"));
}

#[test]
fn nested_structured_result_wrappers_are_fully_unwrapped() {
    let payload = json!({
        "content": [{"type": "text", "text": "completed"}],
        "_structured_result": {
            "_structured_result": {
                "_structured_result": {
                    "changed": true,
                    "sha256": "abc"
                }
            }
        }
    });

    let (text, structured) = to_text_and_structured_result(&payload);

    assert_eq!(text, "completed");
    assert_eq!(structured, Some(json!({"changed": true, "sha256": "abc"})));
    assert_eq!(
        structured_result_payload(&payload),
        &json!({"changed": true, "sha256": "abc"})
    );
}

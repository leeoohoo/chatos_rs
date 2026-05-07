use serde_json::json;

use super::actions_console::{build_browser_console_eval_summary, build_browser_console_summary};
use super::actions_console_support::{build_console_message_counts, summarize_json_value_inline};
use super::actions_research_payloads::build_browser_extract_results_brief;
use super::actions_research_text::{
    build_browser_research_findings, build_browser_research_summary,
};
use super::actions_shared::{
    build_browser_inspect_summary, has_meaningful_page_signal, mark_page_state_available,
};
use super::actions_vision::{
    ai_model_config_to_runtime_value, build_browser_vision_chat_messages,
    build_browser_vision_responses_input, build_browser_vision_unavailable_message,
    preferred_browser_vision_transport, BrowserVisionTransport,
};
use super::actions_vision_support::{model_cfg_supports_browser_vision, BrowserVisionCandidate};
use crate::builtin::browser_page_insights::is_meaningful_browser_url;
use crate::builtin::web_tools::provider::ExtractedPage;
use crate::models::ai_model_config::AiModelConfig;

#[test]
fn browser_console_summary_includes_page_error_and_clear_state() {
    let response = json!({
        "title": "Dashboard",
        "url": "https://example.com/app",
        "total_messages": 2,
        "total_errors": 1,
        "clear_applied": true,
        "errors_brief": [
            { "message_preview": "Uncaught TypeError: x is not a function" }
        ]
    });

    let summary = build_browser_console_summary(&response);
    assert!(summary.contains("Collected 2 console message(s) and 1 JavaScript error(s)"));
    assert!(summary.contains("Current page: Dashboard [https://example.com/app]."));
    assert!(summary.contains("Latest JS error: Uncaught TypeError: x is not a function."));
    assert!(summary.contains("Console buffers were cleared after reading."));
}

#[test]
fn browser_console_eval_summary_mentions_preview() {
    let response = json!({
        "title": "Pricing",
        "url": "https://example.com/pricing",
        "result_type": "object",
        "result_preview": "object keys: plan, price"
    });

    let summary = build_browser_console_eval_summary(&response);
    assert!(summary.contains("Evaluated JavaScript in the current page. Result type: object."));
    assert!(summary.contains("Current page: Pricing [https://example.com/pricing]."));
    assert!(summary.contains("Result preview: object keys: plan, price."));
}

#[test]
fn browser_inspect_summary_mentions_snapshot_console_and_vision() {
    let response = json!({
        "title": "Checkout",
        "url": "https://example.com/checkout",
        "element_count": 18,
        "total_messages": 3,
        "total_errors": 1,
        "vision": {
            "enabled": true,
            "mode": "user_model",
            "model": "gpt-4o",
            "transport": "chat_completions"
        }
    });

    let summary = build_browser_inspect_summary(&response, true);
    assert!(summary.contains("Observed the current browser page."));
    assert!(summary.contains("Current page: Checkout [https://example.com/checkout]."));
    assert!(summary.contains("Visible refs in snapshot: 18."));
    assert!(summary.contains("Console summary: 3 message(s), 1 JavaScript error(s)."));
    assert!(summary.contains(
        "Vision answered the inspection question via user_model / gpt-4o over chat_completions."
    ));
}

#[test]
fn browser_inspect_summary_flags_missing_active_page() {
    let response = json!({
        "url": "about:blank",
        "page_state_available": false,
        "inspection_warning": "page: no active browser page was available; open a page before running browser_inspect",
    });

    let summary = build_browser_inspect_summary(&response, true);
    assert!(summary.contains("No active browser page was available."));
    assert!(summary.contains("Vision inspection was skipped because no active page was open."));
    assert!(!summary.contains("about:blank"));
}

#[test]
fn page_state_ignores_placeholder_url_without_other_signals() {
    let mut response = json!({
        "url": "about:blank",
    });

    mark_page_state_available(&mut response);
    assert_eq!(
        response
            .get("page_state_available")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert!(!has_meaningful_page_signal(&response));
    assert!(!is_meaningful_browser_url("about:blank"));
    assert!(is_meaningful_browser_url("https://example.com"));
}

#[test]
fn browser_research_summary_mentions_page_and_web_context() {
    let response = json!({
        "question": "What does this pricing page say?",
        "include_web": true,
        "web_query": "example pricing competitors",
        "page": {
            "title": "Pricing",
            "url": "https://example.com/pricing",
            "inspection_steps": {
                "snapshot": "ok",
                "console": "ok",
                "vision": "ok"
            }
        },
        "research_summary": {
            "search_backend": "chatos_native_search",
            "search_result_count": 4,
            "extract_backend": "chatos_native_extract",
            "extracted_page_count": 2,
            "selected_url_count": 2,
            "total_omitted_chars": 900
        },
        "research_warning": "web_extract: fallback used"
    });

    let summary = build_browser_research_summary(&response);
    assert!(
        summary.contains("Researched the current browser page for \"What does this pricing page say?\".")
    );
    assert!(summary.contains("Current page: Pricing [https://example.com/pricing]."));
    assert!(summary.contains("Page inspect steps: snapshot=ok, console=ok, vision=ok."));
    assert!(summary.contains(
        "Supplemental web research for \"example pricing competitors\" returned 4 result(s) and extracted 2 page(s)"
    ));
    assert!(summary.contains("Research warning: web_extract: fallback used."));
}

#[test]
fn browser_research_findings_include_page_web_and_sources() {
    let response = json!({
        "question": "What changed on this docs page?",
        "include_web": true,
        "web_query": "example docs release notes",
        "page": {
            "success": true,
            "title": "Docs",
            "url": "https://example.com/docs",
            "element_count": 12,
            "inspection_steps": {
                "snapshot": "ok",
                "console": "ok",
                "vision": "ok"
            },
            "total_messages": 2,
            "total_errors": 1,
            "messages_brief": [
                { "text_preview": "Deprecated API warning from the docs script" }
            ],
            "errors_brief": [
                { "message_preview": "Uncaught TypeError: window.docsInit is not a function" }
            ],
            "analysis": "The page emphasizes the new browser research workflow and links to release notes."
        },
        "research_summary": {
            "page_success": true,
            "search_backend": "chatos_native_search",
            "search_result_count": 4,
            "selected_url_count": 2,
            "extract_backend": "chatos_native_extract",
            "extracted_page_count": 2,
            "truncated_page_count": 1,
            "total_omitted_chars": 900
        },
        "search": {
            "results_brief": [
                {
                    "title": "Release notes",
                    "url": "https://example.com/release-notes",
                    "description_preview": "Explains the docs navigation update."
                }
            ]
        },
        "extract": {
            "results_brief": [
                {
                    "title": "Release notes",
                    "url": "https://example.com/release-notes",
                    "status": "ok, truncated",
                    "content_preview": "Details the browser research and inspect improvements."
                }
            ]
        },
        "research_warning": "web_extract: fallback used"
    });

    let findings = build_browser_research_findings(&response);
    assert_eq!(
        findings.get("answer_frame").and_then(|value| value.as_str()),
        Some(
            "Combined page and web research for \"What changed on this docs page?\" completed. Page inspect: usable. External search found 4 result(s) and extracted 2 page(s)."
        )
    );
    assert!(findings
        .get("page_findings")
        .and_then(|value| value.as_array())
        .is_some_and(|items| items.iter().any(|item| item
            .as_str()
            .is_some_and(|text| text.contains("Snapshot exposed 12 visible ref(s)")))));
    assert!(findings
        .get("page_findings")
        .and_then(|value| value.as_array())
        .is_some_and(|items| items.iter().any(|item| item
            .as_str()
            .is_some_and(|text| text.contains("Latest JS error: Uncaught TypeError")))));
    assert!(findings
        .get("web_findings")
        .and_then(|value| value.as_array())
        .is_some_and(|items| items.iter().any(|item| item.as_str().is_some_and(
            |text| text.contains("Key extracted sources: Release notes (ok, truncated).")
        ))));
    assert_eq!(
        findings
            .get("source_highlights")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("kind"))
            .and_then(|value| value.as_str()),
        Some("extract")
    );
    assert!(findings
        .get("recommended_next_steps")
        .and_then(|value| value.as_array())
        .is_some_and(|items| items.iter().any(|item| item
            .as_str()
            .is_some_and(|text| text.contains("JavaScript errors")))));
}

#[test]
fn browser_extract_results_brief_hides_error_rows_when_success_content_exists() {
    let items = build_browser_extract_results_brief(&[
        ExtractedPage {
            url: "https://example.com/a".to_string(),
            title: "A".to_string(),
            content: "Useful extracted content".to_string(),
            content_chars: 22,
            original_content_chars: 22,
            truncated: false,
            content_summary: None,
            error: None,
        },
        ExtractedPage {
            url: "https://example.com/b".to_string(),
            title: "B".to_string(),
            content: String::new(),
            content_chars: 0,
            original_content_chars: 0,
            truncated: false,
            content_summary: None,
            error: Some("request timed out".to_string()),
        },
    ]);

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].get("url").and_then(|value| value.as_str()),
        Some("https://example.com/a")
    );
}

#[test]
fn console_message_counts_group_by_type() {
    let counts = build_console_message_counts(&[
        json!({"type": "log"}),
        json!({"type": "warn"}),
        json!({"type": "warn"}),
    ]);

    assert_eq!(counts.get("log").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(counts.get("warn").and_then(|v| v.as_u64()), Some(2));
}

#[test]
fn summarize_json_value_inline_compacts_objects_and_arrays() {
    assert_eq!(
        summarize_json_value_inline(&json!({"foo": 1, "bar": 2}), 120),
        "object keys: foo, bar"
    );
    assert_eq!(
        summarize_json_value_inline(&json!([1, "a", true]), 120),
        "array(3 items: number, string, bool)"
    );
}

#[test]
fn ai_model_config_to_runtime_value_uses_model_name_field() {
    let value = ai_model_config_to_runtime_value(&AiModelConfig {
        id: "model_1".to_string(),
        name: "Vision".to_string(),
        provider: "gpt".to_string(),
        model: "gpt-4o-mini".to_string(),
        thinking_level: Some("medium".to_string()),
        api_key: Some("key".to_string()),
        base_url: Some("https://api.openai.com/v1".to_string()),
        user_id: Some("user_1".to_string()),
        enabled: true,
        supports_images: true,
        supports_reasoning: true,
        supports_responses: true,
        created_at: String::new(),
        updated_at: String::new(),
    });

    assert_eq!(
        value.get("model_name").and_then(|v| v.as_str()),
        Some("gpt-4o-mini")
    );
    assert_eq!(value.get("model").and_then(|v| v.as_str()), None);
    assert_eq!(
        value.get("supports_responses").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn browser_vision_support_detection_accepts_known_vision_models() {
    assert!(model_cfg_supports_browser_vision(&json!({}), "gpt-4o"));
    assert!(model_cfg_supports_browser_vision(
        &json!({"supports_images": true}),
        "custom-model"
    ));
}

#[test]
fn browser_vision_unavailable_message_includes_warnings() {
    let text = build_browser_vision_unavailable_message(&[
        "missing contact context".to_string(),
        "no fallback key".to_string(),
    ]);
    assert!(text.contains("browser_vision has no available vision-capable model configuration."));
    assert!(text.contains("missing contact context"));
    assert!(text.contains("no fallback key"));
}

#[test]
fn browser_vision_responses_input_uses_input_image_parts() {
    let input = build_browser_vision_responses_input(
        "What is on the page?",
        "data:image/png;base64,abc",
    );
    let content = input
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("content"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap();

    assert_eq!(content.len(), 2);
    assert_eq!(
        content[0].get("type").and_then(|v| v.as_str()),
        Some("input_text")
    );
    assert_eq!(
        content[1].get("type").and_then(|v| v.as_str()),
        Some("input_image")
    );
    assert_eq!(
        content[1].get("image_url").and_then(|v| v.as_str()),
        Some("data:image/png;base64,abc")
    );
}

#[test]
fn browser_vision_chat_messages_use_chat_multimodal_shape() {
    let messages = build_browser_vision_chat_messages(
        "What is on the page?",
        "data:image/png;base64,abc",
        Some("You are a helpful analyst."),
        false,
    );

    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].get("role").and_then(|v| v.as_str()),
        Some("system")
    );
    assert_eq!(
        messages[1].get("role").and_then(|v| v.as_str()),
        Some("user")
    );

    let content = messages[1]
        .get("content")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap();
    assert_eq!(
        content[0].get("type").and_then(|v| v.as_str()),
        Some("text")
    );
    assert_eq!(
        content[1].get("type").and_then(|v| v.as_str()),
        Some("image_url")
    );
    assert_eq!(
        content[1]
            .get("image_url")
            .and_then(|value| value.get("url"))
            .and_then(|v| v.as_str()),
        Some("data:image/png;base64,abc")
    );
}

#[test]
fn browser_vision_transport_prefers_responses_when_supported() {
    let candidate = BrowserVisionCandidate {
        mode: "user_model",
        prompt_source: "generic",
        contact_agent_id: None,
        instructions: None,
        model: "gpt-4o".to_string(),
        provider: "gpt".to_string(),
        thinking_level: None,
        temperature: 0.7,
        api_key: "key".to_string(),
        base_url: "https://api.openai.com/v1".to_string(),
        supports_responses: true,
    };

    assert_eq!(
        preferred_browser_vision_transport(&candidate),
        BrowserVisionTransport::Responses
    );
}

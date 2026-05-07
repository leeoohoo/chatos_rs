use serde_json::{json, Value};

use crate::builtin::browser_page_insights::{
    latest_console_text_line, latest_js_error_line, page_label_from_response,
    page_state_warning_line,
};
use super::actions_console_support::{
    build_console_message_counts, build_console_messages_brief, build_js_errors_brief,
    result_type_name, summarize_json_value_inline,
};
use super::actions_shared::{
    browser_error_message, build_browser_action_summary, enrich_response_with_page_metadata,
    fail_json, is_success, normalize_inline_text, parse_browser_eval_payload,
    run_browser_command,
};
use super::BoundContext;

pub(super) async fn browser_console_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    clear: bool,
    expression: Option<String>,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
    if let Some(expression) = expression {
        let result = run_browser_command(
            &ctx,
            session.as_str(),
            "eval",
            vec![expression],
            ctx.command_timeout_seconds,
        )
        .await?;
        if !is_success(&result) {
            return Ok(fail_json(&result, "eval failed"));
        }

        let raw = result
            .get("data")
            .and_then(|v| v.get("result"))
            .cloned()
            .unwrap_or(Value::Null);
        let parsed = parse_browser_eval_payload(raw);
        let result_type = result_type_name(&parsed);
        let result_preview = summarize_json_value_inline(&parsed, 220);
        let mut response = json!({
            "success": true,
            "result": parsed,
            "result_type": result_type,
            "result_preview": result_preview,
        });
        enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
        response["_summary_text"] = Value::String(build_browser_console_eval_summary(&response));
        return Ok(response);
    }

    let mut console_args = Vec::new();
    if clear {
        console_args.push("--clear".to_string());
    }
    let console_result = run_browser_command(
        &ctx,
        session.as_str(),
        "console",
        console_args.clone(),
        ctx.command_timeout_seconds,
    )
    .await?;
    let errors_result = run_browser_command(
        &ctx,
        session.as_str(),
        "errors",
        console_args,
        ctx.command_timeout_seconds,
    )
    .await?;

    let console_ok = is_success(&console_result);
    let errors_ok = is_success(&errors_result);
    if !console_ok && !errors_ok {
        let combined_error = [
            browser_error_message(&console_result, "console output unavailable"),
            browser_error_message(&errors_result, "JavaScript errors unavailable"),
        ]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
        let summary_action = format!(
            "Browser console inspection failed: {}.",
            normalize_inline_text(combined_error.as_str(), 180)
        );
        let mut response = json!({
            "success": false,
            "error": combined_error,
        });
        enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
        response["_summary_text"] = Value::String(build_browser_action_summary(
            summary_action.as_str(),
            &response,
            None,
        ));
        return Ok(response);
    }

    let mut messages: Vec<Value> = Vec::new();
    if console_ok {
        if let Some(arr) = console_result
            .get("data")
            .and_then(|v| v.get("messages"))
            .and_then(|v| v.as_array())
        {
            for item in arr {
                let typ = item.get("type").and_then(|v| v.as_str()).unwrap_or("log");
                let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
                messages.push(json!({
                    "type": typ,
                    "text": text,
                    "source": "console"
                }));
            }
        }
    }

    let mut errors: Vec<Value> = Vec::new();
    if errors_ok {
        if let Some(arr) = errors_result
            .get("data")
            .and_then(|v| v.get("errors"))
            .and_then(|v| v.as_array())
        {
            for item in arr {
                let text = item.get("message").and_then(|v| v.as_str()).unwrap_or("");
                errors.push(json!({
                    "message": text,
                    "source": "exception"
                }));
            }
        }
    }

    let mut warnings = Vec::new();
    if !console_ok {
        warnings.push(browser_error_message(
            &console_result,
            "console output unavailable",
        ));
    }
    if !errors_ok {
        warnings.push(browser_error_message(
            &errors_result,
            "JavaScript errors unavailable",
        ));
    }

    let messages_brief = build_console_messages_brief(messages.as_slice(), 5);
    let errors_brief = build_js_errors_brief(errors.as_slice(), 5);
    let message_count_by_type = build_console_message_counts(messages.as_slice());
    let total_messages = messages.len();
    let total_errors = errors.len();
    let mut response = json!({
        "success": true,
        "clear_applied": clear,
        "messages_brief": messages_brief,
        "errors_brief": errors_brief,
        "message_count_by_type": message_count_by_type,
        "total_messages": total_messages,
        "total_errors": total_errors,
        "console_messages": messages,
        "js_errors": errors,
    });
    if !warnings.is_empty() {
        response["console_warning"] = Value::String(warnings.join(" | "));
    }
    enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
    response["_summary_text"] = Value::String(build_browser_console_summary(&response));

    Ok(response)
}

pub(super) fn build_browser_console_summary(response: &Value) -> String {
    let total_messages = response
        .get("total_messages")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total_errors = response
        .get("total_errors")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let mut parts = vec![format!(
        "Collected {} console message(s) and {} JavaScript error(s) from the current page.",
        total_messages, total_errors,
    )];

    let page_label = page_label_from_response(response);
    if !page_label.is_empty() {
        parts.push(page_label);
    }

    if let Some(line) = latest_js_error_line(response, "Latest JS error") {
        parts.push(line);
    } else if let Some(line) = latest_console_text_line(response, "Latest console message") {
        parts.push(line);
    }

    if response
        .get("clear_applied")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        parts.push("Console buffers were cleared after reading.".to_string());
    }

    if let Some(warning) = response
        .get("console_warning")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Console collection warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }
    if let Some(line) = page_state_warning_line(response) {
        parts.push(line);
    }

    parts.join(" ")
}

pub(super) fn build_browser_console_eval_summary(response: &Value) -> String {
    let result_type = response
        .get("result_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let mut parts = vec![format!(
        "Evaluated JavaScript in the current page. Result type: {}.",
        result_type
    )];

    let page_label = page_label_from_response(response);
    if !page_label.is_empty() {
        parts.push(page_label);
    }

    if let Some(preview) = response
        .get("result_preview")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Result preview: {}.",
            normalize_inline_text(preview, 180)
        ));
    }
    if let Some(line) = page_state_warning_line(response) {
        parts.push(line);
    }

    parts.join(" ")
}

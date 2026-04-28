use serde_json::{json, Value};

use super::actions_shared::{
    browser_inspect_warning, copy_response_fields, summarize_browser_failure,
};

pub(super) fn merge_snapshot_result(
    response: &mut Value,
    snapshot: &Value,
    warnings: &mut Vec<String>,
) -> &'static str {
    let success = snapshot
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    copy_response_fields(
        response,
        snapshot,
        &[
            "url",
            "title",
            "snapshot",
            "element_count",
            "page_state_available",
            "page_state_warning",
        ],
    );
    if !success {
        warnings.push(browser_inspect_warning(
            "snapshot",
            summarize_browser_failure(snapshot, "snapshot unavailable").as_str(),
        ));
    }
    if success { "ok" } else { "error" }
}

pub(super) fn merge_console_result(
    response: &mut Value,
    console: &Value,
    warnings: &mut Vec<String>,
) -> &'static str {
    let success = console
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    copy_response_fields(
        response,
        console,
        &[
            "clear_applied",
            "messages_brief",
            "errors_brief",
            "message_count_by_type",
            "total_messages",
            "total_errors",
            "console_messages",
            "js_errors",
            "console_warning",
        ],
    );
    if !success {
        warnings.push(browser_inspect_warning(
            "console",
            summarize_browser_failure(console, "console inspection unavailable").as_str(),
        ));
    }
    if success { "ok" } else { "error" }
}

pub(super) fn merge_vision_result(
    response: &mut Value,
    vision: &Value,
    warnings: &mut Vec<String>,
) -> &'static str {
    let enabled = vision
        .get("vision")
        .and_then(|value| value.get("enabled"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    copy_response_fields(
        response,
        vision,
        &["analysis", "question", "screenshot_path", "annotations", "vision"],
    );
    if !enabled {
        warnings.push(browser_inspect_warning(
            "vision",
            summarize_browser_failure(vision, "vision inspection unavailable").as_str(),
        ));
    }
    if enabled { "ok" } else { "error" }
}

pub(super) fn set_inspection_steps(
    response: &mut Value,
    snapshot_status: &str,
    console_status: &str,
    vision_status: &str,
) {
    response["inspection_steps"] = json!({
        "snapshot": snapshot_status,
        "console": console_status,
        "vision": vision_status,
    });
}

pub(super) fn set_inspection_warning(response: &mut Value, warnings: &[String]) {
    if !warnings.is_empty() {
        response["inspection_warning"] = Value::String(warnings.join(" | "));
    }
}

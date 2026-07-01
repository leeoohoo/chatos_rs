// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use chatos_mcp_runtime::ToolCallerModelRuntime;

use super::actions_inspect_support::{
    merge_console_result, merge_snapshot_result, merge_vision_result, set_inspection_steps,
    set_inspection_warning,
};
use super::actions_shared::{
    browser_inspect_warning, build_browser_inspect_summary, has_console_signal,
    has_meaningful_page_signal,
};
use super::{
    browser_console_with_context, browser_snapshot_with_context, browser_vision_with_context,
    BoundContext,
};

pub(super) async fn browser_inspect_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    caller_model_runtime: Option<ToolCallerModelRuntime>,
    question: Option<String>,
    full: bool,
    annotate: bool,
) -> Result<Value, String> {
    let mut response = json!({
        "success": true,
        "inspection_mode": "read_only_observe",
        "full_snapshot": full,
    });
    let mut warnings = Vec::new();
    let mut snapshot_status = "error";
    let mut console_status = "error";
    let vision_requested = question.is_some();
    let mut vision_status = if vision_requested { "error" } else { "skipped" };

    match browser_snapshot_with_context(ctx.clone(), conversation_id, full).await {
        Ok(snapshot) => {
            snapshot_status = merge_snapshot_result(&mut response, &snapshot, &mut warnings);
        }
        Err(err) => warnings.push(browser_inspect_warning("snapshot", err.as_str())),
    }

    match browser_console_with_context(ctx.clone(), conversation_id, false, None).await {
        Ok(console) => {
            console_status = merge_console_result(&mut response, &console, &mut warnings);
        }
        Err(err) => warnings.push(browser_inspect_warning("console", err.as_str())),
    }

    let has_page_signal_before_vision = has_meaningful_page_signal(&response);
    let has_console_signal_before_vision = has_console_signal(&response);
    if !has_page_signal_before_vision && !has_console_signal_before_vision {
        warnings.push(browser_inspect_warning(
            "page",
            "no active browser page was available; open a page before running browser_inspect",
        ));
    }

    if let Some(question) = question {
        if has_page_signal_before_vision {
            match browser_vision_with_context(
                ctx,
                conversation_id,
                caller_model_runtime,
                question,
                annotate,
            )
            .await
            {
                Ok(vision) => {
                    vision_status = merge_vision_result(&mut response, &vision, &mut warnings);
                }
                Err(err) => warnings.push(browser_inspect_warning("vision", err.as_str())),
            }
        } else {
            vision_status = "skipped";
            warnings.push(browser_inspect_warning(
                "vision",
                "skipped because no active browser page was available",
            ));
        }
    }

    let any_success = has_meaningful_page_signal(&response) || has_console_signal(&response);

    response["success"] = Value::Bool(any_success);
    set_inspection_steps(
        &mut response,
        snapshot_status,
        console_status,
        vision_status,
    );
    set_inspection_warning(&mut response, &warnings);
    response["_summary_text"] =
        Value::String(build_browser_inspect_summary(&response, vision_requested));

    Ok(response)
}

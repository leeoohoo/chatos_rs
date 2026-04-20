use std::collections::HashSet;

use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine as _;
use serde_json::{json, Value};
use tokio::time::Duration;
use uuid::Uuid;

use crate::builtin::browser_runtime::{
    new_browser_session, run_browser_command as runtime_run_browser_command, BrowserRuntimeSession,
};
use crate::builtin::web_tools::provider::{
    extract_with_fallback, search_with_fallback, select_research_extract_urls,
    BrowserRenderOptions, ExtractedPage, SearchHit,
};
use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::chat_runtime::{compose_contact_system_prompt, ChatRuntimeMetadata};
use crate::models::ai_model_config::AiModelConfig;
use crate::repositories::ai_model_configs;
use crate::services::memory_server_client;
use crate::services::v2::ai_request_handler as v2_ai_request_handler;
use crate::services::v2::message_manager as v2_message_manager;
use crate::services::v3::ai_request_handler as v3_ai_request_handler;
use crate::services::v3::message_manager as v3_message_manager;
use crate::utils::attachments::is_vision_model;

use super::BoundContext;

const SCROLL_PIXELS: i32 = 500;
const DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS: i64 = 700;
const DEFAULT_BROWSER_RESEARCH_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_BROWSER_RESEARCH_LIMIT: usize = 5;
const MAX_BROWSER_RESEARCH_LIMIT: usize = 20;
const MAX_BROWSER_RESEARCH_EXTRACT_URLS: usize = 5;
const DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS: usize = 100_000;

pub(super) async fn browser_navigate_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    url: String,
) -> Result<Value, String> {
    let session = super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "open",
        vec![url.clone()],
        ctx.command_timeout_seconds.max(60),
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Navigation failed"));
    }

    let data = result.get("data").cloned().unwrap_or_else(|| json!({}));
    let final_url = data
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or(url.as_str())
        .to_string();
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut response = json!({
        "success": true,
        "url": final_url,
        "title": title
    });
    enrich_response_with_page_state(&ctx, session.as_str(), &mut response, false).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Opened page.",
        &response,
        Some("Use refs from the snapshot with browser_click or browser_type."),
    ));

    Ok(response)
}

pub(super) async fn browser_snapshot_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    full: bool,
) -> Result<Value, String> {
    let session = super::context::conversation_key(conversation_id);
    let args = if full { vec![] } else { vec!["-c".to_string()] };
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "snapshot",
        args,
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to get snapshot"));
    }

    let data = result.get("data").cloned().unwrap_or_else(|| json!({}));
    let mut response = json!({
        "success": true,
    });
    apply_snapshot_payload(&mut response, &data, ctx.max_snapshot_chars);
    enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Captured page snapshot.",
        &response,
        Some("Use refs like @e12 from the snapshot when clicking or typing."),
    ));
    Ok(response)
}

pub(super) async fn browser_inspect_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
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
            let success = snapshot
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            snapshot_status = if success { "ok" } else { "error" };
            copy_response_fields(
                &mut response,
                &snapshot,
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
                    summarize_browser_failure(&snapshot, "snapshot unavailable").as_str(),
                ));
            }
        }
        Err(err) => warnings.push(browser_inspect_warning("snapshot", err.as_str())),
    }

    match browser_console_with_context(ctx.clone(), conversation_id, false, None).await {
        Ok(console) => {
            let success = console
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            console_status = if success { "ok" } else { "error" };
            copy_response_fields(
                &mut response,
                &console,
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
                    summarize_browser_failure(&console, "console inspection unavailable").as_str(),
                ));
            }
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
            match browser_vision_with_context(ctx, conversation_id, question, annotate).await {
                Ok(vision) => {
                    let enabled = vision
                        .get("vision")
                        .and_then(|value| value.get("enabled"))
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    vision_status = if enabled { "ok" } else { "error" };
                    copy_response_fields(
                        &mut response,
                        &vision,
                        &[
                            "analysis",
                            "question",
                            "screenshot_path",
                            "annotations",
                            "vision",
                        ],
                    );
                    if !enabled {
                        warnings.push(browser_inspect_warning(
                            "vision",
                            summarize_browser_failure(&vision, "vision inspection unavailable")
                                .as_str(),
                        ));
                    }
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
    response["inspection_steps"] = json!({
        "snapshot": snapshot_status,
        "console": console_status,
        "vision": vision_status,
    });
    if !warnings.is_empty() {
        response["inspection_warning"] = Value::String(warnings.join(" | "));
    }
    response["_summary_text"] =
        Value::String(build_browser_inspect_summary(&response, vision_requested));

    Ok(response)
}

pub(super) async fn browser_research_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    question: String,
    web_query: Option<String>,
    include_web: bool,
    web_limit: Option<usize>,
    extract_top: Option<usize>,
    full: bool,
    annotate: bool,
) -> Result<Value, String> {
    let mut response = json!({
        "success": true,
        "research_mode": if include_web { "page_plus_web" } else { "page_only" },
        "question": question.clone(),
        "include_web": include_web,
    });
    let mut warnings = Vec::new();

    let page = browser_inspect_with_context(
        ctx.clone(),
        conversation_id,
        Some(question.clone()),
        full,
        annotate,
    )
    .await?;
    let page_success = page
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    response["page"] = page.clone();
    if !page_success {
        warnings.push(format!(
            "page: {}",
            summarize_browser_failure(&page, "page inspection unavailable")
        ));
    }

    let mut selected_urls: Vec<String> = Vec::new();
    let mut research_summary = json!({
        "page_success": page_success,
        "search_backend": "none",
        "search_fallback_used": false,
        "search_result_count": 0,
        "selected_url_count": 0,
        "extract_backend": "none",
        "extract_fallback_used": false,
        "extracted_page_count": 0,
        "truncated_page_count": 0,
        "total_original_chars": 0,
        "total_returned_chars": 0,
        "total_omitted_chars": 0,
    });

    if include_web {
        let query = web_query
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| question.clone());
        let search_limit = web_limit
            .unwrap_or(DEFAULT_BROWSER_RESEARCH_LIMIT)
            .clamp(1, MAX_BROWSER_RESEARCH_LIMIT);
        response["web_query"] = Value::String(query.clone());

        match reqwest::Client::builder()
            .timeout(Duration::from_secs(
                DEFAULT_BROWSER_RESEARCH_REQUEST_TIMEOUT_SECONDS,
            ))
            .user_agent("chatos-rs-browser-research/0.1")
            .build()
        {
            Ok(client) => match search_with_fallback(
                &client,
                query.as_str(),
                search_limit,
                Some(&BrowserRenderOptions {
                    workspace_dir: ctx.workspace_dir.clone(),
                    command_timeout_seconds: ctx.command_timeout_seconds,
                }),
            )
            .await
            {
                Ok(search_outcome) => {
                    let search_result_count = search_outcome.hits.len();
                    let search_had_hits = !search_outcome.hits.is_empty();
                    let search_results_brief =
                        build_browser_research_results_brief(search_outcome.hits.as_slice());
                    let desired_extract_count = extract_top
                        .unwrap_or(search_limit.min(3))
                        .min(MAX_BROWSER_RESEARCH_EXTRACT_URLS);
                    selected_urls = select_research_extract_urls(
                        search_outcome.hits.as_slice(),
                        desired_extract_count,
                        MAX_BROWSER_RESEARCH_EXTRACT_URLS,
                    );

                    research_summary["search_backend"] =
                        Value::String(search_outcome.backend.clone());
                    research_summary["search_fallback_used"] =
                        Value::Bool(search_outcome.fallback_used);
                    research_summary["search_result_count"] = json!(search_result_count);
                    response["search"] = json!({
                        "backend": search_outcome.backend,
                        "fallback_used": search_outcome.fallback_used,
                        "provider_attempts": search_outcome.attempts,
                        "result_count": search_result_count,
                        "results_brief": search_results_brief,
                        "data": {
                            "web": search_outcome.hits
                        }
                    });

                    research_summary["selected_url_count"] = json!(selected_urls.len());

                    if selected_urls.is_empty() {
                        if desired_extract_count == 0 {
                            warnings.push(
                                "web_extract: extraction skipped because extract_top was set to 0."
                                    .to_string(),
                            );
                        } else if !search_had_hits {
                            warnings.push(
                                    "web_extract: no search hits were returned, so nothing was extracted."
                                        .to_string(),
                                );
                        }
                        response["extract"] = browser_research_empty_extract_payload();
                    } else {
                        match extract_with_fallback(
                            &client,
                            &selected_urls,
                            DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS,
                            Some(&BrowserRenderOptions {
                                workspace_dir: ctx.workspace_dir.clone(),
                                command_timeout_seconds: ctx.command_timeout_seconds,
                            }),
                        )
                        .await
                        {
                            Ok(extract_outcome) => {
                                let page_count = extract_outcome.pages.len();
                                let truncated_page_count = extract_outcome
                                    .pages
                                    .iter()
                                    .filter(|page| page.truncated)
                                    .count();
                                let total_original_chars: usize = extract_outcome
                                    .pages
                                    .iter()
                                    .map(|page| page.original_content_chars)
                                    .sum();
                                let total_returned_chars: usize = extract_outcome
                                    .pages
                                    .iter()
                                    .map(|page| page.content_chars)
                                    .sum();
                                let total_omitted_chars =
                                    total_original_chars.saturating_sub(total_returned_chars);

                                research_summary["extract_backend"] =
                                    Value::String(extract_outcome.backend.clone());
                                research_summary["extract_fallback_used"] =
                                    Value::Bool(extract_outcome.fallback_used);
                                research_summary["extracted_page_count"] = json!(page_count);
                                research_summary["truncated_page_count"] =
                                    json!(truncated_page_count);
                                research_summary["total_original_chars"] =
                                    json!(total_original_chars);
                                research_summary["total_returned_chars"] =
                                    json!(total_returned_chars);
                                research_summary["total_omitted_chars"] =
                                    json!(total_omitted_chars);

                                response["extract"] = json!({
                                    "backend": extract_outcome.backend,
                                    "fallback_used": extract_outcome.fallback_used,
                                    "provider_attempts": extract_outcome.attempts,
                                    "results_brief": build_browser_extract_results_brief(extract_outcome.pages.as_slice()),
                                    "extract_summary": {
                                        "max_extract_chars_per_page": DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS,
                                        "page_count": page_count,
                                        "truncated_page_count": truncated_page_count,
                                        "total_original_chars": total_original_chars,
                                        "total_returned_chars": total_returned_chars,
                                        "total_omitted_chars": total_omitted_chars
                                    },
                                    "results": extract_outcome.pages
                                });
                            }
                            Err(err) => {
                                warnings.push(format!("web_extract: {}", err));
                                response["extract"] = browser_research_empty_extract_payload();
                            }
                        }
                    }
                }
                Err(err) => {
                    warnings.push(format!("web_search: {}", err));
                    response["search"] = json!({
                        "backend": "none",
                        "fallback_used": false,
                        "provider_attempts": [],
                        "result_count": 0,
                        "results_brief": [],
                        "data": { "web": [] }
                    });
                    response["extract"] = browser_research_empty_extract_payload();
                }
            },
            Err(err) => {
                warnings.push(format!(
                    "web_search: build browser_research client failed: {}",
                    err
                ));
                response["search"] = json!({
                    "backend": "none",
                    "fallback_used": false,
                    "provider_attempts": [],
                    "result_count": 0,
                    "results_brief": [],
                    "data": { "web": [] }
                });
                response["extract"] = browser_research_empty_extract_payload();
            }
        }
    } else {
        warnings.push("external web research was skipped because include_web=false".to_string());
    }

    response["selected_urls"] = json!(selected_urls);
    if !warnings.is_empty() {
        response["research_warning"] = Value::String(warnings.join(" | "));
    }
    research_summary["warning"] = response
        .get("research_warning")
        .cloned()
        .unwrap_or(Value::Null);
    response["research_summary"] = research_summary;
    response["research_findings"] = build_browser_research_findings(&response);

    let web_success = response
        .get("search")
        .and_then(|value| value.get("result_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        > 0
        || response
            .get("extract")
            .and_then(|value| value.get("extract_summary"))
            .and_then(|value| value.get("page_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0;
    response["success"] = Value::Bool(page_success || web_success);
    response["_summary_text"] = Value::String(build_browser_research_summary(&response));

    Ok(response)
}

pub(super) async fn browser_click_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    mut reference: String,
) -> Result<Value, String> {
    if !reference.starts_with('@') {
        reference = format!("@{}", reference.trim());
    }
    let session = super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "click",
        vec![reference.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to click {}", reference).as_str(),
        ));
    }

    let mut response = json!({
        "success": true,
        "clicked": reference
    });
    enrich_response_with_page_state(&ctx, session.as_str(), &mut response, false).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Clicked element.",
        &response,
        None,
    ));
    Ok(response)
}

pub(super) async fn browser_type_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    mut reference: String,
    text: String,
) -> Result<Value, String> {
    if !reference.starts_with('@') {
        reference = format!("@{}", reference.trim());
    }
    let session = super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "fill",
        vec![reference.clone(), text.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to type into {}", reference).as_str(),
        ));
    }

    let mut response = json!({
        "success": true,
        "typed": text,
        "typed_chars": text.chars().count(),
        "element": reference
    });
    enrich_response_with_page_state(&ctx, session.as_str(), &mut response, false).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Typed into element.",
        &response,
        None,
    ));
    Ok(response)
}

pub(super) async fn browser_scroll_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    direction: String,
) -> Result<Value, String> {
    if direction != "up" && direction != "down" {
        return Ok(json!({
            "_summary_text": format!("Browser scroll failed because direction '{}' is invalid.", direction),
            "success": false,
            "error": format!("Invalid direction '{}'. Use 'up' or 'down'.", direction)
        }));
    }
    let session = super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "scroll",
        vec![direction.clone(), SCROLL_PIXELS.to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to scroll {}", direction).as_str(),
        ));
    }

    let mut response = json!({
        "success": true,
        "scrolled": direction
    });
    enrich_response_with_page_state(&ctx, session.as_str(), &mut response, false).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Scrolled page.",
        &response,
        None,
    ));
    Ok(response)
}

pub(super) async fn browser_back_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "back",
        vec![],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to go back"));
    }

    let url = result
        .get("data")
        .and_then(|data| data.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut response = json!({
        "success": true,
        "url": url,
    });
    enrich_response_with_page_state(&ctx, session.as_str(), &mut response, false).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Navigated back in history.",
        &response,
        None,
    ));
    Ok(response)
}

pub(super) async fn browser_press_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    key: String,
) -> Result<Value, String> {
    let session = super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "press",
        vec![key.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to press {}", key).as_str(),
        ));
    }
    let mut response = json!({
        "success": true,
        "pressed": key
    });
    enrich_response_with_page_state(&ctx, session.as_str(), &mut response, false).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Pressed keyboard key.",
        &response,
        None,
    ));
    Ok(response)
}

pub(super) async fn browser_console_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    clear: bool,
    expression: Option<String>,
) -> Result<Value, String> {
    let session = super::context::conversation_key(conversation_id);
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
        let parsed = if let Some(text) = raw.as_str() {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
        } else {
            raw
        };
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

pub(super) async fn browser_get_images_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::context::conversation_key(conversation_id);
    let js = r#"JSON.stringify(
        [...document.images].map(img => ({
            src: img.src,
            alt: img.alt || '',
            width: img.naturalWidth,
            height: img.naturalHeight
        })).filter(img => img.src && !img.src.startsWith('data:'))
    )"#;
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "eval",
        vec![js.to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to get images"));
    }
    let raw = result
        .get("data")
        .and_then(|v| v.get("result"))
        .cloned()
        .unwrap_or_else(|| Value::String("[]".to_string()));
    let parsed = if let Some(text) = raw.as_str() {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!([]))
    } else {
        raw
    };
    let count = parsed.as_array().map(|v| v.len()).unwrap_or(0);
    Ok(json!({
        "_summary_text": format!("Found {} image(s) in the current page DOM.", count),
        "success": true,
        "images": parsed,
        "count": count
    }))
}

pub(super) async fn browser_vision_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    question: String,
    annotate: bool,
) -> Result<Value, String> {
    let session = super::context::conversation_key(conversation_id);
    let screenshot_dir = ctx
        .workspace_dir
        .join(".chatos")
        .join("browser_screenshots");
    std::fs::create_dir_all(&screenshot_dir)
        .map_err(|err| format!("create screenshot dir failed: {}", err))?;
    let screenshot_path = screenshot_dir.join(format!(
        "browser_screenshot_{}.png",
        Uuid::new_v4().simple()
    ));
    let mut args = Vec::new();
    if annotate {
        args.push("--annotate".to_string());
    }
    args.push("--full".to_string());
    args.push(screenshot_path.to_string_lossy().to_string());

    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "screenshot",
        args,
        ctx.command_timeout_seconds.max(60),
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to take screenshot"));
    }

    let actual_path = result
        .get("data")
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| screenshot_path.to_string_lossy().to_string());

    let (analysis, vision) = match analyze_screenshot_with_best_available_runtime(
        question.as_str(),
        actual_path.as_str(),
        conversation_id,
    )
    .await
    {
        Ok(output) => (
            output.analysis,
            json!({
                "enabled": true,
                "mode": output.mode,
                "prompt_source": output.prompt_source,
                "contact_agent_id": output.contact_agent_id,
                "model": output.model,
                "provider": output.provider,
                "transport": output.transport,
                "fallback_used": output.fallback_used,
                "transport_fallback_used": output.transport_fallback_used,
                "attempts": output.attempts,
                "warnings": output.warnings,
            }),
        ),
        Err(err) => (
            "Screenshot captured, but vision analysis was unavailable. See vision.error and vision.attempts.".to_string(),
            json!({
                "enabled": false,
                "mode": "unavailable",
                "error": err.error,
                "attempts": err.attempts,
                "warnings": err.warnings,
            }),
        ),
    };

    Ok(json!({
        "_summary_text": format!(
            "Captured a browser screenshot and produced vision analysis (vision available: {}, mode: {}, transport: {}).",
            if vision.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                "yes"
            } else {
                "no"
            },
            vision.get("mode").and_then(|v| v.as_str()).unwrap_or("unknown"),
            vision
                .get("transport")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        ),
        "success": true,
        "analysis": analysis,
        "question": question,
        "screenshot_path": actual_path,
        "annotations": result.get("data").and_then(|v| v.get("annotations")).cloned().unwrap_or(Value::Null),
        "vision": vision,
    }))
}

#[derive(Debug, Clone)]
struct BrowserVisionPreparedContext {
    session_model_cfg: Option<Value>,
    selected_model_id: Option<String>,
    user_id: Option<String>,
    contact_agent_id: Option<String>,
    contact_system_prompt: Option<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct BrowserVisionCandidate {
    mode: &'static str,
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
    model: String,
    provider: String,
    thinking_level: Option<String>,
    temperature: f64,
    api_key: String,
    base_url: String,
    supports_responses: bool,
}

#[derive(Debug, Clone)]
struct BrowserVisionOutput {
    analysis: String,
    mode: String,
    prompt_source: String,
    contact_agent_id: Option<String>,
    model: String,
    provider: String,
    transport: String,
    fallback_used: bool,
    transport_fallback_used: bool,
    attempts: Vec<Value>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct BrowserVisionFailure {
    error: String,
    attempts: Vec<Value>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserVisionTransport {
    Responses,
    ChatCompletions,
}

impl BrowserVisionTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat_completions",
        }
    }
}

#[derive(Debug, Clone)]
struct BrowserVisionRunResult {
    analysis: String,
    transport: &'static str,
    transport_fallback_used: bool,
}

async fn analyze_screenshot_with_best_available_runtime(
    question: &str,
    screenshot_path: &str,
    conversation_id: Option<&str>,
) -> Result<BrowserVisionOutput, BrowserVisionFailure> {
    let prepared = prepare_browser_vision_context(conversation_id).await;
    let mut warnings = prepared.warnings.clone();
    let candidates = build_browser_vision_candidates(&prepared, &mut warnings).await;
    if candidates.is_empty() {
        return Err(BrowserVisionFailure {
            error: build_browser_vision_unavailable_message(warnings.as_slice()),
            attempts: Vec::new(),
            warnings,
        });
    }

    let image_bytes =
        tokio::fs::read(screenshot_path)
            .await
            .map_err(|err| BrowserVisionFailure {
                error: format!("read screenshot failed: {}", err),
                attempts: Vec::new(),
                warnings: warnings.clone(),
            })?;
    let mime = mime_guess::from_path(screenshot_path).first_or_octet_stream();
    let image_data_url = format!(
        "data:{};base64,{}",
        mime.essence_str(),
        BASE64_STD.encode(image_bytes)
    );
    let prompt = build_browser_vision_prompt(question);
    let total_candidates = candidates.len();
    let mut attempts = Vec::new();
    let mut last_error = String::new();

    for (index, candidate) in candidates.into_iter().enumerate() {
        match run_browser_vision_candidate(prompt.as_str(), image_data_url.as_str(), &candidate)
            .await
        {
            Ok(run_result) => {
                let attempt_provider = candidate.provider.clone();
                let attempt_model = candidate.model.clone();
                attempts.push(json!({
                    "mode": candidate.mode,
                    "prompt_source": candidate.prompt_source,
                    "provider": attempt_provider,
                    "model": attempt_model,
                    "transport": run_result.transport,
                    "transport_fallback_used": run_result.transport_fallback_used,
                    "status": "success"
                }));
                return Ok(BrowserVisionOutput {
                    analysis: run_result.analysis,
                    mode: candidate.mode.to_string(),
                    prompt_source: candidate.prompt_source.to_string(),
                    contact_agent_id: candidate.contact_agent_id.clone(),
                    model: candidate.model,
                    provider: candidate.provider,
                    transport: run_result.transport.to_string(),
                    fallback_used: index > 0,
                    transport_fallback_used: run_result.transport_fallback_used,
                    attempts,
                    warnings,
                });
            }
            Err(err) => {
                last_error = err.clone();
                let attempt_provider = candidate.provider.clone();
                let attempt_model = candidate.model.clone();
                attempts.push(json!({
                    "mode": candidate.mode,
                    "prompt_source": candidate.prompt_source,
                    "provider": attempt_provider,
                    "model": attempt_model,
                    "transport": preferred_browser_vision_transport(&candidate).as_str(),
                    "status": "error",
                    "error": normalize_inline_text(err.as_str(), 220)
                }));
            }
        }
    }

    Err(BrowserVisionFailure {
        error: format!(
            "vision analysis failed for all {} candidate(s). Last error: {}",
            total_candidates,
            normalize_inline_text(last_error.as_str(), 220)
        ),
        attempts,
        warnings,
    })
}

async fn prepare_browser_vision_context(
    conversation_id: Option<&str>,
) -> BrowserVisionPreparedContext {
    let mut context = BrowserVisionPreparedContext {
        session_model_cfg: None,
        selected_model_id: None,
        user_id: None,
        contact_agent_id: None,
        contact_system_prompt: None,
        warnings: Vec::new(),
    };

    let Some(conversation_id) = normalize_non_empty(conversation_id) else {
        context.warnings.push(
            "No active conversation_id was available, so browser_vision will skip session/contact context."
                .to_string(),
        );
        return context;
    };

    let session = match memory_server_client::get_session_by_id(conversation_id.as_str()).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            context
                .warnings
                .push(format!("conversation not found: {}", conversation_id));
            return context;
        }
        Err(err) => {
            context
                .warnings
                .push(format!("load current session failed: {}", err));
            return context;
        }
    };

    context.user_id = normalize_non_empty(session.user_id.as_deref());
    context.selected_model_id = normalize_non_empty(session.selected_model_id.as_deref());

    if context.selected_model_id.is_some() {
        match load_session_model_cfg_value(&session).await {
            Ok(value) if !json_value_is_empty_object(&value) => {
                context.session_model_cfg = Some(value);
            }
            Ok(_) => context.warnings.push(
                "Current session has a selected model id, but the model config could not be loaded."
                    .to_string(),
            ),
            Err(err) => context
                .warnings
                .push(format!("load current session model config failed: {}", err)),
        }
    }

    let metadata_runtime = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    context.contact_agent_id = normalize_non_empty(session.selected_agent_id.as_deref())
        .or_else(|| metadata_runtime.contact_agent_id.clone());

    if let Some(contact_agent_id) = context.contact_agent_id.clone() {
        match memory_server_client::get_memory_agent_runtime_context(contact_agent_id.as_str())
            .await
        {
            Ok(Some(runtime)) => {
                context.contact_system_prompt =
                    normalize_non_empty(
                        compose_contact_system_prompt(
                            Some(&runtime),
                            &crate::core::chat_runtime::ContactSkillPromptMode::Disabled,
                        )
                        .as_deref(),
                    );
            }
            Ok(None) => context.warnings.push(format!(
                "contact runtime context not found for agent {}",
                contact_agent_id
            )),
            Err(err) => context
                .warnings
                .push(format!("load contact runtime context failed: {}", err)),
        }
    } else {
        context.warnings.push(
            "Current session has no selected contact agent, so browser_vision will use a generic prompt."
                .to_string(),
        );
    }

    context
}

async fn build_browser_vision_candidates(
    prepared: &BrowserVisionPreparedContext,
    warnings: &mut Vec<String>,
) -> Vec<BrowserVisionCandidate> {
    let prompt_source = if prepared.contact_system_prompt.is_some() {
        "contact_agent"
    } else {
        "generic"
    };
    let instructions = prepared.contact_system_prompt.clone();
    let contact_agent_id = prepared.contact_agent_id.clone();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if let Some(model_cfg) = prepared.session_model_cfg.as_ref() {
        if let Some(candidate) = browser_vision_candidate_from_model_cfg(
            model_cfg,
            "session_model",
            prompt_source,
            contact_agent_id.clone(),
            instructions.clone(),
        ) {
            push_browser_vision_candidate(&mut out, &mut seen, candidate);
        } else {
            warnings.push(
                "Current session model is unavailable for browser_vision, so a fallback model will be used."
                    .to_string(),
            );
        }
    }

    if let Some(user_id) = prepared.user_id.as_deref() {
        match ai_model_configs::list_ai_model_configs(Some(user_id.to_string())).await {
            Ok(configs) => {
                for model_cfg in configs.into_iter().filter(|cfg| cfg.enabled) {
                    if prepared.selected_model_id.as_deref() == Some(model_cfg.id.as_str()) {
                        continue;
                    }
                    let value = ai_model_config_to_runtime_value(&model_cfg);
                    if let Some(candidate) = browser_vision_candidate_from_model_cfg(
                        &value,
                        "user_model",
                        prompt_source,
                        contact_agent_id.clone(),
                        instructions.clone(),
                    ) {
                        push_browser_vision_candidate(&mut out, &mut seen, candidate);
                    }
                }
            }
            Err(err) => warnings.push(format!(
                "list enabled image-capable model configs failed: {}",
                err
            )),
        }
    }

    if let Some(candidate) =
        default_browser_vision_candidate(prompt_source, contact_agent_id, instructions)
    {
        push_browser_vision_candidate(&mut out, &mut seen, candidate);
    } else {
        warnings.push(
            "No global OPENAI_API_KEY fallback is configured for browser_vision.".to_string(),
        );
    }

    out
}

fn browser_vision_candidate_from_model_cfg(
    model_cfg: &Value,
    mode: &'static str,
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
) -> Option<BrowserVisionCandidate> {
    let cfg = Config::get();
    let runtime = resolve_chat_model_config(
        model_cfg,
        "gpt-4o",
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        Some(true),
        true,
    );
    if runtime.api_key.trim().is_empty() || runtime.base_url.trim().is_empty() {
        return None;
    }
    if !model_cfg_supports_browser_vision(model_cfg, runtime.model.as_str()) {
        return None;
    }

    Some(BrowserVisionCandidate {
        mode,
        prompt_source,
        contact_agent_id,
        instructions,
        model: runtime.model,
        provider: runtime.provider,
        thinking_level: runtime.thinking_level,
        temperature: runtime.temperature,
        api_key: runtime.api_key,
        base_url: runtime.base_url,
        supports_responses: model_cfg
            .get("supports_responses")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    })
}

fn default_browser_vision_candidate(
    prompt_source: &'static str,
    contact_agent_id: Option<String>,
    instructions: Option<String>,
) -> Option<BrowserVisionCandidate> {
    let cfg = Config::get();
    if cfg.openai_api_key.trim().is_empty() || cfg.openai_base_url.trim().is_empty() {
        return None;
    }

    Some(BrowserVisionCandidate {
        mode: "default_model",
        prompt_source,
        contact_agent_id,
        instructions,
        model: "gpt-4o".to_string(),
        provider: "gpt".to_string(),
        thinking_level: None,
        temperature: 0.7,
        api_key: cfg.openai_api_key.clone(),
        base_url: cfg.openai_base_url.clone(),
        supports_responses: true,
    })
}

fn push_browser_vision_candidate(
    out: &mut Vec<BrowserVisionCandidate>,
    seen: &mut HashSet<String>,
    candidate: BrowserVisionCandidate,
) {
    let signature = format!(
        "{}|{}|{}",
        candidate.provider, candidate.model, candidate.base_url
    );
    if seen.insert(signature) {
        out.push(candidate);
    }
}

fn model_cfg_supports_browser_vision(model_cfg: &Value, resolved_model: &str) -> bool {
    model_cfg
        .get("supports_images")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || is_vision_model(resolved_model)
}

fn build_browser_vision_prompt(question: &str) -> String {
    format!(
        "你现在收到了一张当前网页截图。请仅基于截图内容回答用户问题，先给结论，再给1-3条关键依据。用户问题：{}",
        question
    )
}

fn build_browser_vision_responses_input(prompt: &str, image_data_url: &str) -> Value {
    json!([
        {
            "type": "message",
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": prompt
                },
                {
                    "type": "input_image",
                    "image_url": image_data_url
                }
            ]
        }
    ])
}

fn build_browser_vision_chat_messages(
    prompt: &str,
    image_data_url: &str,
    system_prompt: Option<&str>,
    no_system_messages: bool,
) -> Vec<Value> {
    let wrapped_prompt =
        build_browser_vision_wrapped_prompt(prompt, system_prompt, no_system_messages);
    let user_content = json!([
        {
            "type": "text",
            "text": wrapped_prompt
        },
        {
            "type": "image_url",
            "image_url": {
                "url": image_data_url
            }
        }
    ]);

    if no_system_messages {
        return vec![json!({
            "role": "user",
            "content": user_content
        })];
    }

    let mut messages = Vec::new();
    if let Some(system_prompt) = system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": user_content
    }));
    messages
}

fn build_browser_vision_wrapped_prompt(
    prompt: &str,
    system_prompt: Option<&str>,
    inline_system_context: bool,
) -> String {
    if !inline_system_context {
        return prompt.to_string();
    }

    let Some(system_prompt) = system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return prompt.to_string();
    };

    format!("【系统上下文】\n{}\n\n{}", system_prompt, prompt)
}

async fn run_browser_vision_candidate(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<BrowserVisionRunResult, String> {
    let preferred_transport = preferred_browser_vision_transport(candidate);
    match run_browser_vision_candidate_once(prompt, image_data_url, candidate, preferred_transport)
        .await
    {
        Ok(analysis) => Ok(BrowserVisionRunResult {
            analysis,
            transport: preferred_transport.as_str(),
            transport_fallback_used: false,
        }),
        Err(primary_err) => {
            let Some(fallback_transport) = fallback_browser_vision_transport(preferred_transport)
            else {
                return Err(primary_err);
            };

            match run_browser_vision_candidate_once(
                prompt,
                image_data_url,
                candidate,
                fallback_transport,
            )
            .await
            {
                Ok(analysis) => Ok(BrowserVisionRunResult {
                    analysis,
                    transport: fallback_transport.as_str(),
                    transport_fallback_used: true,
                }),
                Err(fallback_err) => Err(format!(
                    "{} transport failed: {}; {} fallback failed: {}",
                    preferred_transport.as_str(),
                    normalize_inline_text(primary_err.as_str(), 220),
                    fallback_transport.as_str(),
                    normalize_inline_text(fallback_err.as_str(), 220)
                )),
            }
        }
    }
}

fn preferred_browser_vision_transport(
    candidate: &BrowserVisionCandidate,
) -> BrowserVisionTransport {
    if candidate.supports_responses {
        BrowserVisionTransport::Responses
    } else {
        BrowserVisionTransport::ChatCompletions
    }
}

fn fallback_browser_vision_transport(
    transport: BrowserVisionTransport,
) -> Option<BrowserVisionTransport> {
    match transport {
        BrowserVisionTransport::Responses => Some(BrowserVisionTransport::ChatCompletions),
        BrowserVisionTransport::ChatCompletions => None,
    }
}

async fn run_browser_vision_candidate_once(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
    transport: BrowserVisionTransport,
) -> Result<String, String> {
    match transport {
        BrowserVisionTransport::Responses => {
            run_browser_vision_with_responses(prompt, image_data_url, candidate).await
        }
        BrowserVisionTransport::ChatCompletions => {
            run_browser_vision_with_chat_completions(prompt, image_data_url, candidate).await
        }
    }
}

async fn run_browser_vision_with_responses(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<String, String> {
    let handler = v3_ai_request_handler::AiRequestHandler::new(
        candidate.api_key.clone(),
        candidate.base_url.clone(),
        v3_message_manager::MessageManager::new(),
    );
    let no_system_messages =
        browser_vision_base_url_disallows_system_messages(candidate.base_url.as_str());
    let wrapped_prompt = build_browser_vision_wrapped_prompt(
        prompt,
        candidate.instructions.as_deref(),
        no_system_messages,
    );
    let input = build_browser_vision_responses_input(wrapped_prompt.as_str(), image_data_url);
    let response = handler
        .handle_request(
            input,
            candidate.model.clone(),
            if no_system_messages {
                None
            } else {
                candidate.instructions.clone()
            },
            None,
            None,
            None,
            Some(candidate.temperature),
            Some(DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS),
            v3_ai_request_handler::StreamCallbacks::default(),
            Some(candidate.provider.clone()),
            candidate.thinking_level.clone(),
            None,
            None,
            false,
            None,
            None,
            if candidate.prompt_source == "contact_agent" {
                "browser_vision_contact"
            } else {
                "browser_vision_fallback"
            },
        )
        .await
        .map_err(|err| format!("responses transport request failed: {}", err))?;
    let analysis = select_browser_vision_response_text(response.content, response.reasoning);
    if analysis.trim().is_empty() {
        return Err("responses transport did not include text output".to_string());
    }
    Ok(analysis)
}

async fn run_browser_vision_with_chat_completions(
    prompt: &str,
    image_data_url: &str,
    candidate: &BrowserVisionCandidate,
) -> Result<String, String> {
    let handler = v2_ai_request_handler::AiRequestHandler::new(
        candidate.api_key.clone(),
        candidate.base_url.clone(),
        v2_message_manager::MessageManager::new(),
    );
    let no_system_messages =
        browser_vision_base_url_disallows_system_messages(candidate.base_url.as_str());
    let messages = build_browser_vision_chat_messages(
        prompt,
        image_data_url,
        candidate.instructions.as_deref(),
        no_system_messages,
    );
    let response = handler
        .handle_request(
            messages,
            None,
            candidate.model.clone(),
            Some(candidate.temperature),
            Some(DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS),
            v2_ai_request_handler::StreamCallbacks {
                on_chunk: None,
                on_thinking: None,
            },
            false,
            Some(candidate.provider.clone()),
            candidate.thinking_level.clone(),
            None,
            None,
            false,
            None,
            None,
            if candidate.prompt_source == "contact_agent" {
                "browser_vision_contact"
            } else {
                "browser_vision_fallback"
            },
        )
        .await
        .map_err(|err| format!("chat/completions transport request failed: {}", err))?;
    let analysis = select_browser_vision_response_text(response.content, response.reasoning);
    if analysis.trim().is_empty() {
        return Err("chat/completions transport did not include text output".to_string());
    }
    Ok(analysis)
}

fn browser_vision_base_url_disallows_system_messages(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    if let Ok(value) = std::env::var("DISABLE_SYSTEM_MESSAGES_FOR_PROXY") {
        let normalized = value.trim().to_lowercase();
        return normalized == "1"
            || normalized == "true"
            || normalized == "yes"
            || normalized == "on";
    }

    false
}

fn select_browser_vision_response_text(content: String, reasoning: Option<String>) -> String {
    if !content.trim().is_empty() {
        return content.trim().to_string();
    }

    if let Some(reasoning) = reasoning {
        if !reasoning.trim().is_empty() {
            return reasoning.trim().to_string();
        }
    }

    String::new()
}

fn build_browser_vision_unavailable_message(warnings: &[String]) -> String {
    if warnings.is_empty() {
        "browser_vision has no available vision-capable model configuration.".to_string()
    } else {
        format!(
            "browser_vision has no available vision-capable model configuration. {}",
            warnings
                .iter()
                .map(|item| normalize_inline_text(item.as_str(), 180))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    }
}

fn ai_model_config_to_runtime_value(model_cfg: &AiModelConfig) -> Value {
    json!({
        "id": model_cfg.id,
        "name": model_cfg.name,
        "provider": model_cfg.provider,
        "model_name": model_cfg.model,
        "thinking_level": model_cfg.thinking_level,
        "api_key": model_cfg.api_key,
        "base_url": model_cfg.base_url,
        "user_id": model_cfg.user_id,
        "enabled": model_cfg.enabled,
        "supports_images": model_cfg.supports_images,
        "supports_reasoning": model_cfg.supports_reasoning,
        "supports_responses": model_cfg.supports_responses,
    })
}

fn json_value_is_empty_object(value: &Value) -> bool {
    value
        .as_object()
        .map(|items| items.is_empty())
        .unwrap_or(false)
}

async fn load_session_model_cfg_value(
    session: &crate::models::session::Session,
) -> Result<Value, String> {
    let Some(model_id) = normalize_non_empty(session.selected_model_id.as_deref()) else {
        return Ok(json!({}));
    };
    let Some(model_cfg) = ai_model_configs::get_ai_model_config_by_id(model_id.as_str()).await?
    else {
        return Ok(json!({}));
    };
    Ok(ai_model_config_to_runtime_value(&model_cfg))
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

async fn enrich_response_with_page_state(
    ctx: &BoundContext,
    conversation_key: &str,
    response: &mut Value,
    full_snapshot: bool,
) {
    enrich_response_with_page_metadata(ctx, conversation_key, response).await;

    let snapshot_args = if full_snapshot {
        Vec::new()
    } else {
        vec!["-c".to_string()]
    };
    let snapshot_result = run_browser_command(
        ctx,
        conversation_key,
        "snapshot",
        snapshot_args,
        ctx.command_timeout_seconds,
    )
    .await;
    match snapshot_result {
        Ok(value) if is_success(&value) => {
            let data = value.get("data").cloned().unwrap_or_else(|| json!({}));
            apply_snapshot_payload(response, &data, ctx.max_snapshot_chars);
        }
        Ok(value) => {
            append_page_state_warning(response, browser_error_message(&value, "snapshot failed"))
        }
        Err(err) => append_page_state_warning(response, err),
    }

    mark_page_state_available(response);
}

async fn enrich_response_with_page_metadata(
    ctx: &BoundContext,
    conversation_key: &str,
    response: &mut Value,
) {
    let metadata_result = run_browser_command(
        ctx,
        conversation_key,
        "eval",
        vec![current_page_metadata_expression()],
        ctx.command_timeout_seconds,
    )
    .await;

    match metadata_result {
        Ok(value) if is_success(&value) => {
            let raw = value
                .get("data")
                .and_then(|v| v.get("result"))
                .cloned()
                .unwrap_or(Value::Null);
            let parsed = parse_browser_eval_payload(raw);
            if let Some(url) = parsed.get("url").and_then(|v| v.as_str()) {
                upsert_string_field(response, "url", url);
            }
            if let Some(title) = parsed.get("title").and_then(|v| v.as_str()) {
                upsert_string_field(response, "title", title);
            }
        }
        Ok(value) => append_page_state_warning(
            response,
            browser_error_message(&value, "page metadata unavailable"),
        ),
        Err(err) => append_page_state_warning(response, err),
    }

    mark_page_state_available(response);
}

fn current_page_metadata_expression() -> String {
    r#"JSON.stringify({url: window.location.href, title: document.title})"#.to_string()
}

fn parse_browser_eval_payload(raw: Value) -> Value {
    if let Some(text) = raw.as_str() {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
    } else {
        raw
    }
}

fn apply_snapshot_payload(response: &mut Value, data: &Value, max_snapshot_chars: usize) {
    let snapshot = data.get("snapshot").and_then(|v| v.as_str()).unwrap_or("");
    let refs = data.get("refs").and_then(|v| v.as_object());
    upsert_string_field(
        response,
        "snapshot",
        truncate_chars(snapshot, max_snapshot_chars).as_str(),
    );
    if let Some(map) = response.as_object_mut() {
        map.insert(
            "element_count".to_string(),
            json!(refs.map(|v| v.len()).unwrap_or(0)),
        );
    }
}

fn upsert_string_field(response: &mut Value, key: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    if let Some(map) = response.as_object_mut() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn append_page_state_warning(response: &mut Value, warning: String) {
    let warning = warning.trim();
    if warning.is_empty() {
        return;
    }
    if let Some(map) = response.as_object_mut() {
        let merged = match map.get("page_state_warning").and_then(|v| v.as_str()) {
            Some(existing) if !existing.trim().is_empty() => {
                format!("{} | {}", existing.trim(), warning)
            }
            _ => warning.to_string(),
        };
        map.insert("page_state_warning".to_string(), Value::String(merged));
    }
}

fn mark_page_state_available(response: &mut Value) {
    if let Some(map) = response.as_object_mut() {
        let available = map
            .get("url")
            .and_then(|v| v.as_str())
            .map(is_meaningful_browser_url)
            .unwrap_or(false)
            || map
                .get("title")
                .and_then(|v| v.as_str())
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            || map
                .get("snapshot")
                .and_then(|v| v.as_str())
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
        map.insert("page_state_available".to_string(), Value::Bool(available));
    }
}

fn is_meaningful_browser_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    !matches!(
        normalized.as_str(),
        "about:blank"
            | "about:srcdoc"
            | "about:newtab"
            | "data:,"
            | "chrome://newtab/"
            | "chrome://new-tab-page/"
            | "edge://newtab/"
    )
}

fn has_non_empty_snapshot(response: &Value) -> bool {
    response
        .get("snapshot")
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn has_meaningful_page_signal(response: &Value) -> bool {
    response
        .get("url")
        .and_then(|value| value.as_str())
        .map(is_meaningful_browser_url)
        .unwrap_or(false)
        || response
            .get("title")
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        || has_non_empty_snapshot(response)
}

fn has_console_signal(response: &Value) -> bool {
    if response
        .get("total_messages")
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        > 0
        || response
            .get("total_errors")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0
    {
        return true;
    }

    if response
        .get("messages_brief")
        .and_then(|value| value.as_array())
        .is_some_and(|items| !items.is_empty())
        || response
            .get("errors_brief")
            .and_then(|value| value.as_array())
            .is_some_and(|items| !items.is_empty())
    {
        return true;
    }

    response
        .get("message_count_by_type")
        .and_then(|value| value.as_object())
        .is_some_and(|items| items.values().any(|value| value.as_u64().unwrap_or(0) > 0))
}

fn copy_response_fields(target: &mut Value, source: &Value, fields: &[&str]) {
    let Some(target_map) = target.as_object_mut() else {
        return;
    };
    for field in fields {
        if let Some(value) = source.get(*field) {
            target_map.insert((*field).to_string(), value.clone());
        }
    }
}

fn browser_inspect_warning(source: &str, detail: &str) -> String {
    let normalized = normalize_inline_text(detail, 180);
    if normalized.is_empty() {
        format!("{} unavailable", source)
    } else {
        format!("{}: {}", source, normalized)
    }
}

fn summarize_browser_failure(response: &Value, fallback: &str) -> String {
    if let Some(error) = response
        .get("error")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        return error.to_string();
    }
    response
        .get("_summary_text")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn page_label_from_response(response: &Value) -> String {
    let title = response
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let raw_url = response
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let url = if is_meaningful_browser_url(raw_url) {
        raw_url
    } else {
        ""
    };
    if !title.is_empty() && !url.is_empty() {
        format!(
            "Current page: {} [{}].",
            normalize_inline_text(title, 120),
            normalize_inline_text(url, 180)
        )
    } else if !title.is_empty() {
        format!("Current page title: {}.", normalize_inline_text(title, 120))
    } else if !url.is_empty() {
        format!("Current page URL: {}.", normalize_inline_text(url, 180))
    } else {
        String::new()
    }
}

fn first_non_empty<'a>(primary: &'a str, fallback: &'a str) -> &'a str {
    let primary = primary.trim();
    if !primary.is_empty() {
        primary
    } else {
        fallback.trim()
    }
}

fn build_browser_action_summary(action: &str, response: &Value, next_hint: Option<&str>) -> String {
    let mut parts = vec![action.trim().to_string()];
    let page_label = page_label_from_response(response);
    if !page_label.is_empty() {
        parts.push(page_label);
    }

    if let Some(count) = response.get("element_count").and_then(|v| v.as_u64()) {
        parts.push(format!("Visible refs in snapshot: {}.", count));
    }
    if response
        .get("snapshot")
        .and_then(|v| v.as_str())
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        parts.push("A snapshot of the current page is included.".to_string());
    }
    if let Some(warning) = response.get("page_state_warning").and_then(|v| v.as_str()) {
        if !warning.trim().is_empty() {
            parts.push(format!(
                "Page state warning: {}.",
                normalize_inline_text(warning, 180)
            ));
        }
    }
    if let Some(hint) = next_hint.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(hint.to_string());
    }

    parts.join(" ")
}

fn build_browser_inspect_summary(response: &Value, vision_requested: bool) -> String {
    let has_page_signal = has_meaningful_page_signal(response);
    let has_console = has_console_signal(response);
    let mut parts = vec![if has_page_signal || has_console {
        "Observed the current browser page.".to_string()
    } else {
        "No active browser page was available.".to_string()
    }];

    let page_label = page_label_from_response(response);
    if !page_label.is_empty() {
        parts.push(page_label);
    }

    if let Some(count) = response
        .get("element_count")
        .and_then(|value| value.as_u64())
    {
        parts.push(format!("Visible refs in snapshot: {}.", count));
    }

    let total_messages = response
        .get("total_messages")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let total_errors = response
        .get("total_errors")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    if total_messages > 0 || total_errors > 0 || response.get("message_count_by_type").is_some() {
        parts.push(format!(
            "Console summary: {} message(s), {} JavaScript error(s).",
            total_messages, total_errors
        ));
    }

    if vision_requested {
        if !has_page_signal {
            parts
                .push("Vision inspection was skipped because no active page was open.".to_string());
        } else if let Some(vision) = response.get("vision").and_then(|value| value.as_object()) {
            let enabled = vision
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let mode = vision
                .get("mode")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let model = vision
                .get("model")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let transport = vision
                .get("transport")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            if enabled {
                parts.push(format!(
                    "Vision answered the inspection question via {} / {} over {}.",
                    mode, model, transport
                ));
            } else {
                parts.push("Vision was requested but unavailable.".to_string());
            }
        } else {
            parts.push("Vision was requested but no screenshot analysis was returned.".to_string());
        }
    } else {
        parts.push(
            "Use browser_click/browser_type with snapshot refs, or pass question to browser_inspect when visual layout matters."
                .to_string(),
        );
    }

    if let Some(warning) = response
        .get("inspection_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Inspection warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }

    parts.join(" ")
}

fn build_browser_research_summary(response: &Value) -> String {
    let mut parts = Vec::new();
    let question = response
        .get("question")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 180))
        .filter(|value| !value.is_empty());

    if let Some(question) = question {
        parts.push(format!(
            "Researched the current browser page for \"{}\".",
            question
        ));
    } else {
        parts.push("Researched the current browser page.".to_string());
    }

    let page = response.get("page").unwrap_or(response);
    let page_label = page_label_from_response(page);
    if !page_label.is_empty() {
        parts.push(page_label);
    }

    if let Some(steps) = page
        .get("inspection_steps")
        .and_then(|value| value.as_object())
    {
        let snapshot = steps
            .get("snapshot")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let console = steps
            .get("console")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let vision = steps
            .get("vision")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        parts.push(format!(
            "Page inspect steps: snapshot={}, console={}, vision={}.",
            snapshot, console, vision
        ));
    }

    let include_web = response
        .get("include_web")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if include_web {
        let summary = response
            .get("research_summary")
            .and_then(|value| value.as_object());
        let query = response
            .get("web_query")
            .and_then(|value| value.as_str())
            .map(|value| normalize_inline_text(value, 160))
            .filter(|value| !value.is_empty());
        let search_count = summary
            .and_then(|value| value.get("search_result_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let extract_count = summary
            .and_then(|value| value.get("extracted_page_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let _search_backend = summary
            .and_then(|value| value.get("search_backend"))
            .and_then(|value| value.as_str())
            .unwrap_or("none");
        let _extract_backend = summary
            .and_then(|value| value.get("extract_backend"))
            .and_then(|value| value.as_str())
            .unwrap_or("none");
        let selected_url_count = summary
            .and_then(|value| value.get("selected_url_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let total_omitted_chars = summary
            .and_then(|value| value.get("total_omitted_chars"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);

        match query {
            Some(query) => parts.push(format!(
                "Supplemental web research for \"{}\" returned {} result(s) and extracted {} page(s) (selected URLs: {}, omitted chars: {}).",
                query,
                search_count,
                extract_count,
                selected_url_count,
                total_omitted_chars,
            )),
            None => parts.push(format!(
                "Supplemental web research returned {} result(s) and extracted {} page(s) (selected URLs: {}, omitted chars: {}).",
                search_count,
                extract_count,
                selected_url_count,
                total_omitted_chars,
            )),
        }
    } else {
        parts.push("External web research was skipped.".to_string());
    }

    if let Some(warning) = response
        .get("research_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Research warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }

    parts.join(" ")
}

fn build_browser_research_findings(response: &Value) -> Value {
    let page = response.get("page").unwrap_or(response);
    let research_summary = response
        .get("research_summary")
        .and_then(|value| value.as_object());
    let include_web = response
        .get("include_web")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let question = response
        .get("question")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 160))
        .filter(|value| !value.is_empty());
    let query = response
        .get("web_query")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 160))
        .filter(|value| !value.is_empty());
    let page_success = research_summary
        .and_then(|value| value.get("page_success"))
        .and_then(|value| value.as_bool())
        .or_else(|| page.get("success").and_then(|value| value.as_bool()))
        .unwrap_or(false);
    let search_count = research_summary
        .and_then(|value| value.get("search_result_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let extract_count = research_summary
        .and_then(|value| value.get("extracted_page_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let selected_url_count = research_summary
        .and_then(|value| value.get("selected_url_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let truncated_page_count = research_summary
        .and_then(|value| value.get("truncated_page_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let total_omitted_chars = research_summary
        .and_then(|value| value.get("total_omitted_chars"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let _search_backend = research_summary
        .and_then(|value| value.get("search_backend"))
        .and_then(|value| value.as_str())
        .unwrap_or("none");
    let extract_backend = research_summary
        .and_then(|value| value.get("extract_backend"))
        .and_then(|value| value.as_str())
        .unwrap_or("none");
    let total_messages = page
        .get("total_messages")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let total_errors = page
        .get("total_errors")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    let answer_frame = if include_web {
        let focus = question
            .as_deref()
            .map(|value| format!(" for \"{}\"", value))
            .unwrap_or_default();
        format!(
            "Combined page and web research{} {} Page inspect: {}. External search found {} result(s) and extracted {} page(s).",
            focus,
            if page_success { "completed." } else { "completed with a partial page signal." },
            if page_success { "usable" } else { "degraded" },
            search_count,
            extract_count,
        )
    } else {
        let focus = question
            .as_deref()
            .map(|value| format!(" for \"{}\"", value))
            .unwrap_or_default();
        format!(
            "Page-only research{} {} Page inspect returned a {} result.",
            focus,
            if page_success {
                "completed."
            } else {
                "completed with degraded context."
            },
            if page_success { "usable" } else { "partial" },
        )
    };

    let mut page_findings = Vec::new();
    let page_label = page_label_from_response(page);
    if !page_label.is_empty() {
        push_unique_text(&mut page_findings, page_label);
    }
    if let Some(count) = page.get("element_count").and_then(|value| value.as_u64()) {
        push_unique_text(
            &mut page_findings,
            format!(
                "Snapshot exposed {} visible ref(s) that can be reused with browser_click or browser_type.",
                count
            ),
        );
    }
    if let Some(steps) = page
        .get("inspection_steps")
        .and_then(|value| value.as_object())
    {
        let snapshot = steps
            .get("snapshot")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let console = steps
            .get("console")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let vision = steps
            .get("vision")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        push_unique_text(
            &mut page_findings,
            format!(
                "Inspection steps finished with snapshot={}, console={}, vision={}.",
                snapshot, console, vision
            ),
        );
    }
    if total_messages > 0 || total_errors > 0 || page.get("message_count_by_type").is_some() {
        push_unique_text(
            &mut page_findings,
            format!(
                "Console inspection captured {} message(s) and {} JavaScript error(s).",
                total_messages, total_errors
            ),
        );
    }
    if let Some(preview) = page
        .get("errors_brief")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("message_preview"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        push_unique_text(
            &mut page_findings,
            format!("Latest JS error: {}.", normalize_inline_text(preview, 180)),
        );
    }
    if let Some(preview) = page
        .get("messages_brief")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("text_preview"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        push_unique_text(
            &mut page_findings,
            format!(
                "Latest console note: {}.",
                normalize_inline_text(preview, 180)
            ),
        );
    }
    if let Some(analysis) = page
        .get("analysis")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        push_unique_text(
            &mut page_findings,
            format!("Vision take: {}.", normalize_inline_text(analysis, 220)),
        );
    }
    if let Some(warning) = page
        .get("inspection_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        push_unique_text(
            &mut page_findings,
            format!(
                "Inspection warning: {}.",
                normalize_inline_text(warning, 180)
            ),
        );
    }
    if let Some(warning) = page
        .get("page_state_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        push_unique_text(
            &mut page_findings,
            format!(
                "Page state warning: {}.",
                normalize_inline_text(warning, 180)
            ),
        );
    }

    let mut web_findings = Vec::new();
    if include_web {
        let search_focus = query
            .as_deref()
            .or(question.as_deref())
            .map(|value| format!(" for \"{}\"", value))
            .unwrap_or_default();
        push_unique_text(
            &mut web_findings,
            format!(
                "External search{} returned {} result(s).",
                search_focus, search_count
            ),
        );
        let search_titles = response
            .get("search")
            .and_then(|value| value.get("results_brief"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("title")
                            .and_then(|value| value.as_str())
                            .map(|value| normalize_inline_text(value, 100))
                            .filter(|value| !value.is_empty())
                    })
                    .take(3)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !search_titles.is_empty() {
            push_unique_text(
                &mut web_findings,
                format!("Top search hits: {}.", search_titles.join(" | ")),
            );
        }
        if extract_count > 0 || selected_url_count > 0 || !matches!(extract_backend, "none") {
            push_unique_text(
                &mut web_findings,
                format!(
                    "Source extraction reviewed {} selected URL(s) and returned {} page(s); truncated pages: {}, omitted chars: {}.",
                    selected_url_count,
                    extract_count,
                    truncated_page_count,
                    total_omitted_chars,
                ),
            );
        }
        let extracted_titles = response
            .get("extract")
            .and_then(|value| value.get("results_brief"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let title = item
                            .get("title")
                            .and_then(|value| value.as_str())
                            .map(|value| normalize_inline_text(value, 90))
                            .filter(|value| !value.is_empty())?;
                        let status = item
                            .get("status")
                            .and_then(|value| value.as_str())
                            .map(|value| normalize_inline_text(value, 60))
                            .unwrap_or_else(|| "unknown".to_string());
                        Some(format!("{} ({})", title, status))
                    })
                    .take(3)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !extracted_titles.is_empty() {
            push_unique_text(
                &mut web_findings,
                format!("Key extracted sources: {}.", extracted_titles.join(" | ")),
            );
        }
    } else {
        push_unique_text(
            &mut web_findings,
            "External web research was skipped for this run.".to_string(),
        );
    }
    if let Some(warning) = response
        .get("research_warning")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        push_unique_text(
            &mut web_findings,
            format!("Research warning: {}.", normalize_inline_text(warning, 180)),
        );
    }

    let source_highlights = build_research_source_highlights(
        response
            .get("extract")
            .and_then(|value| value.get("results_brief"))
            .and_then(|value| value.as_array()),
        response
            .get("search")
            .and_then(|value| value.get("results_brief"))
            .and_then(|value| value.as_array()),
    );

    let mut recommended_next_steps = Vec::new();
    if !page_success {
        push_unique_text(
            &mut recommended_next_steps,
            "Refresh or reopen the current page, then rerun browser_research to recover full page context.".to_string(),
        );
    }
    if include_web {
        if search_count == 0 {
            push_unique_text(
                &mut recommended_next_steps,
                "Tighten web_query with product, site, or date keywords so web_search can return more specific hits.".to_string(),
            );
        } else if extract_count == 0 {
            push_unique_text(
                &mut recommended_next_steps,
                "Run web_extract on selected_urls or increase extract_top when you need the source text, not just hit titles.".to_string(),
            );
        }
        if truncated_page_count > 0 {
            push_unique_text(
                &mut recommended_next_steps,
                "If one highlighted source matters, re-run extraction on that URL alone so less content is truncated.".to_string(),
            );
        }
    } else {
        push_unique_text(
            &mut recommended_next_steps,
            "Set include_web=true when the answer needs corroboration beyond the current browser page.".to_string(),
        );
    }
    if total_errors > 0 {
        push_unique_text(
            &mut recommended_next_steps,
            "Use browser_console or browser_console_eval to investigate the page's JavaScript errors before trusting dynamic content.".to_string(),
        );
    }
    if response
        .get("research_warning")
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        push_unique_text(
            &mut recommended_next_steps,
            "Review the research_warning field because this run used a degraded path for at least one research step.".to_string(),
        );
    }
    if recommended_next_steps.is_empty() {
        push_unique_text(
            &mut recommended_next_steps,
            "Ask a narrower follow-up question against these findings or open one highlighted source for deeper inspection.".to_string(),
        );
    }

    json!({
        "answer_frame": answer_frame,
        "page_findings": page_findings,
        "web_findings": web_findings,
        "source_highlights": source_highlights,
        "recommended_next_steps": recommended_next_steps,
    })
}

fn build_research_source_highlights(
    extract_results_brief: Option<&Vec<Value>>,
    search_results_brief: Option<&Vec<Value>>,
) -> Vec<Value> {
    if let Some(items) = extract_results_brief.filter(|items| !items.is_empty()) {
        let highlights = items
            .iter()
            .filter_map(build_extract_source_highlight)
            .take(3)
            .collect::<Vec<_>>();
        if !highlights.is_empty() {
            return highlights;
        }
    }

    search_results_brief
        .map(|items| {
            items
                .iter()
                .filter_map(build_search_source_highlight)
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn build_extract_source_highlight(item: &Value) -> Option<Value> {
    let url = item
        .get("url")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let title = item
        .get("title")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 120))
        .unwrap_or_else(|| normalize_inline_text(url, 120));
    let note = item
        .get("content_preview")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 200))
        .unwrap_or_default();
    let status = item
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 80))
        .unwrap_or_else(|| "unknown".to_string());

    if title.is_empty() && url.is_empty() && note.is_empty() {
        return None;
    }

    Some(json!({
        "kind": "extract",
        "title": title,
        "url": url,
        "status": status,
        "note": note,
    }))
}

fn build_search_source_highlight(item: &Value) -> Option<Value> {
    let url = item
        .get("url")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let title = item
        .get("title")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 120))
        .unwrap_or_else(|| normalize_inline_text(url, 120));
    let note = item
        .get("description_preview")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 180))
        .unwrap_or_default();

    if title.is_empty() && url.is_empty() && note.is_empty() {
        return None;
    }

    Some(json!({
        "kind": "search",
        "title": title,
        "url": url,
        "status": "search_hit",
        "note": note,
    }))
}

fn push_unique_text(values: &mut Vec<String>, value: String) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if values.iter().any(|existing| existing == trimmed) {
        return;
    }
    values.push(trimmed.to_string());
}

fn build_browser_research_results_brief(hits: &[SearchHit]) -> Vec<Value> {
    hits.iter()
        .enumerate()
        .map(|(index, hit)| {
            json!({
                "rank": index + 1,
                "title": normalize_inline_text(first_non_empty(hit.title.as_str(), hit.url.as_str()), 120),
                "url": hit.url,
                "description_preview": normalize_inline_text(hit.description.as_str(), 180),
            })
        })
        .collect()
}

fn build_browser_extract_results_brief(pages: &[ExtractedPage]) -> Vec<Value> {
    let show_errors = !pages
        .iter()
        .any(|page| page.error.is_none() && !page.content.trim().is_empty());

    pages.iter()
        .filter(|page| show_errors || page.error.is_none())
        .enumerate()
        .map(|(index, page)| {
            json!({
                "rank": index + 1,
                "title": normalize_inline_text(first_non_empty(page.title.as_str(), page.url.as_str()), 120),
                "url": page.url,
                "status": if let Some(error) = page.error.as_deref() {
                    format!("error: {}", normalize_inline_text(error, 120))
                } else if page.truncated {
                    "ok, truncated".to_string()
                } else {
                    "ok".to_string()
                },
                "content_preview": if let Some(error) = page.error.as_deref() {
                    normalize_inline_text(error, 180)
                } else {
                    normalize_inline_text(page.content.as_str(), 180)
                }
            })
        })
        .collect()
}

fn browser_research_empty_extract_payload() -> Value {
    json!({
        "backend": "none",
        "fallback_used": false,
        "provider_attempts": [],
        "results_brief": [],
        "extract_summary": {
            "max_extract_chars_per_page": DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS,
            "page_count": 0,
            "truncated_page_count": 0,
            "total_original_chars": 0,
            "total_returned_chars": 0,
            "total_omitted_chars": 0
        },
        "results": []
    })
}

fn build_browser_console_summary(response: &Value) -> String {
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

    if let Some(preview) = response
        .get("errors_brief")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("message_preview"))
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Latest JS error: {}.",
            normalize_inline_text(preview, 180)
        ));
    } else if let Some(preview) = response
        .get("messages_brief")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("text_preview"))
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Latest console message: {}.",
            normalize_inline_text(preview, 180)
        ));
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
    if let Some(warning) = response
        .get("page_state_warning")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Page state warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }

    parts.join(" ")
}

fn build_browser_console_eval_summary(response: &Value) -> String {
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
    if let Some(warning) = response
        .get("page_state_warning")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!(
            "Page state warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }

    parts.join(" ")
}

fn build_console_messages_brief(messages: &[Value], max_items: usize) -> Vec<Value> {
    messages
        .iter()
        .take(max_items)
        .map(|item| {
            json!({
                "type": item.get("type").and_then(|v| v.as_str()).unwrap_or("log"),
                "text_preview": normalize_inline_text(
                    item.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                    220
                ),
                "source": item.get("source").and_then(|v| v.as_str()).unwrap_or("console"),
            })
        })
        .collect()
}

fn build_js_errors_brief(errors: &[Value], max_items: usize) -> Vec<Value> {
    errors
        .iter()
        .take(max_items)
        .map(|item| {
            json!({
                "message_preview": normalize_inline_text(
                    item.get("message").and_then(|v| v.as_str()).unwrap_or(""),
                    220
                ),
                "source": item.get("source").and_then(|v| v.as_str()).unwrap_or("exception"),
            })
        })
        .collect()
}

fn build_console_message_counts(messages: &[Value]) -> Value {
    let mut counts = serde_json::Map::new();
    for item in messages {
        let key = item
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("log")
            .trim();
        if key.is_empty() {
            continue;
        }
        let next = counts
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .saturating_add(1);
        counts.insert(key.to_string(), Value::Number(next.into()));
    }
    Value::Object(counts)
}

fn summarize_json_value_inline(value: &Value, max_chars: usize) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => normalize_inline_text(text, max_chars),
        Value::Array(items) => {
            if items.is_empty() {
                "empty array".to_string()
            } else {
                let item_types = items
                    .iter()
                    .take(3)
                    .map(result_type_name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("array({} items: {})", items.len(), item_types)
            }
        }
        Value::Object(map) => {
            if map.is_empty() {
                "empty object".to_string()
            } else {
                let keys = map.keys().take(5).cloned().collect::<Vec<_>>().join(", ");
                format!("object keys: {}", keys)
            }
        }
    }
}

fn normalize_inline_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let total = collapsed.chars().count();
    if total <= max_chars {
        return collapsed;
    }
    let truncated: String = collapsed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect();
    format!("{}...", truncated)
}

async fn run_browser_command(
    ctx: &BoundContext,
    conversation_key: &str,
    command: &str,
    args: Vec<String>,
    timeout_seconds: u64,
) -> Result<Value, String> {
    let session = get_or_create_session(ctx, conversation_key);
    runtime_run_browser_command(&ctx.workspace_dir, &session, command, args, timeout_seconds).await
}

fn get_or_create_session(ctx: &BoundContext, conversation_key: &str) -> BrowserRuntimeSession {
    let mut sessions = ctx.sessions.lock();
    if let Some(existing) = sessions.get(conversation_key) {
        return existing.clone();
    }

    let session = new_browser_session();
    sessions.insert(conversation_key.to_string(), session.clone());
    session
}

fn is_success(value: &Value) -> bool {
    value
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn browser_error_message(value: &Value, fallback: &str) -> String {
    value
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

fn fail_json(value: &Value, fallback: &str) -> Value {
    let error = browser_error_message(value, fallback);
    json!({
        "_summary_text": format!("Browser action failed: {}.", normalize_inline_text(error.as_str(), 180)),
        "success": false,
        "error": error
    })
}

fn result_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ai_model_config_to_runtime_value, build_browser_console_eval_summary,
        build_browser_console_summary, build_browser_extract_results_brief,
        build_browser_inspect_summary, build_browser_research_findings,
        build_browser_research_summary, build_browser_vision_chat_messages,
        build_browser_vision_responses_input, build_browser_vision_unavailable_message,
        build_console_message_counts, has_meaningful_page_signal, is_meaningful_browser_url,
        mark_page_state_available, model_cfg_supports_browser_vision,
        preferred_browser_vision_transport, summarize_json_value_inline, BrowserVisionCandidate,
        BrowserVisionTransport, ExtractedPage,
    };
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
        assert!(summary.contains(
            "Researched the current browser page for \"What does this pricing page say?\"."
        ));
        assert!(summary.contains("Current page: Pricing [https://example.com/pricing]."));
        assert!(summary.contains("Page inspect steps: snapshot=ok, console=ok, vision=ok."));
        assert!(summary.contains("Supplemental web research for \"example pricing competitors\" returned 4 result(s) and extracted 2 page(s)"));
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
            findings
                .get("answer_frame")
                .and_then(|value| value.as_str()),
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
        assert!(
            text.contains("browser_vision has no available vision-capable model configuration.")
        );
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
}

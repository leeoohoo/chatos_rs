use serde_json::Value;

use crate::builtin::research_payloads::{
    build_empty_extract_payload, build_empty_search_payload, build_extract_payload_from_outcome,
    build_search_payload_from_outcome,
};
use super::actions_shared::{
    build_extract_results_brief, build_extract_summary, build_search_results_brief,
    build_search_summary, normalize_extract_urls, normalize_inline_text,
};
use crate::builtin::web_tools::provider::{
    extract_with_fallback, search_with_fallback, BrowserRenderOptions,
};
use super::BoundContext;

pub(super) async fn web_search_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
) -> Result<Value, String> {
    let search_limit = limit
        .unwrap_or(ctx.default_search_limit)
        .clamp(1, ctx.max_search_limit);
    let outcome = match search_with_fallback(
        &ctx.client,
        query.as_str(),
        search_limit,
        Some(&BrowserRenderOptions {
            workspace_dir: ctx.workspace_dir.clone(),
            command_timeout_seconds: ctx.browser_command_timeout_seconds,
        }),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            let mut response = build_empty_search_payload();
            if let Some(map) = response.as_object_mut() {
                map.insert(
                    "_summary_text".to_string(),
                    Value::String(format!(
                        "Web search for \"{}\" could not reach any search provider. {}",
                        normalize_inline_text(query.as_str(), 160),
                        normalize_inline_text(err.as_str(), 220),
                    )),
                );
                map.insert("success".to_string(), Value::Bool(true));
                map.insert("status".to_string(), Value::String("degraded".to_string()));
                map.insert("degraded".to_string(), Value::Bool(true));
                map.insert("warning".to_string(), Value::String(err.clone()));
                map.insert("query".to_string(), Value::String(query));
                map.insert("search_available".to_string(), Value::Bool(false));
                map.insert("error".to_string(), Value::String(err));
            }
            return Ok(response);
        }
    };
    let summary_text = build_search_summary(
        query.as_str(),
        outcome.backend.as_str(),
        outcome.fallback_used,
        outcome.attempts.len(),
        outcome.hits.as_slice(),
    );
    let mut response = build_search_payload_from_outcome(
        &outcome,
        build_search_results_brief(outcome.hits.as_slice()),
    );
    if let Some(map) = response.as_object_mut() {
        map.insert("_summary_text".to_string(), Value::String(summary_text));
        map.insert("success".to_string(), Value::Bool(true));
        map.insert("query".to_string(), Value::String(query));
    }
    Ok(response)
}

pub(super) async fn web_extract_with_context(
    ctx: BoundContext,
    urls: Vec<String>,
) -> Result<Value, String> {
    let normalized = normalize_extract_urls(urls, ctx.max_extract_urls);

    if normalized.is_empty() {
        let mut response = build_empty_extract_payload(ctx.max_extract_chars);
        if let Some(map) = response.as_object_mut() {
            map.insert(
                "_summary_text".to_string(),
                Value::String("No valid URLs were provided, so nothing was extracted.".to_string()),
            );
            map.insert("success".to_string(), Value::Bool(true));
        }
        return Ok(response);
    }

    let outcome = extract_with_fallback(
        &ctx.client,
        &normalized,
        ctx.max_extract_chars,
        Some(&BrowserRenderOptions {
            workspace_dir: ctx.workspace_dir.clone(),
            command_timeout_seconds: ctx.browser_command_timeout_seconds,
        }),
    )
    .await?;
    let stats = crate::builtin::web_tools::provider::compute_research_extract_stats(
        outcome.pages.as_slice(),
    );
    let summary_text = build_extract_summary(
        outcome.backend.as_str(),
        outcome.fallback_used,
        outcome.attempts.len(),
        outcome.pages.as_slice(),
        stats.truncated_page_count,
        stats.total_omitted_chars,
    );
    let mut response = build_extract_payload_from_outcome(
        &outcome,
        build_extract_results_brief(outcome.pages.as_slice()),
        ctx.max_extract_chars,
    );
    if let Some(map) = response.as_object_mut() {
        map.insert("_summary_text".to_string(), Value::String(summary_text));
        map.insert("success".to_string(), Value::Bool(true));
    }
    Ok(response)
}

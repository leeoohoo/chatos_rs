use serde_json::{json, Value};

use super::provider::{extract_with_fallback, search_with_fallback};
use super::BoundContext;

pub(super) async fn web_search_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
) -> Result<Value, String> {
    let search_limit = limit
        .unwrap_or(ctx.default_search_limit)
        .clamp(1, ctx.max_search_limit);
    let outcome = search_with_fallback(&ctx.client, query.as_str(), search_limit).await?;
    Ok(json!({
        "success": true,
        "backend": outcome.backend,
        "fallback_used": outcome.fallback_used,
        "provider_attempts": outcome.attempts,
        "data": {
            "web": outcome.hits
        }
    }))
}

pub(super) async fn web_extract_with_context(
    ctx: BoundContext,
    urls: Vec<String>,
) -> Result<Value, String> {
    let mut normalized = Vec::new();
    for raw in urls {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if normalized.iter().any(|v: &String| v == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
        if normalized.len() >= ctx.max_extract_urls {
            break;
        }
    }

    if normalized.is_empty() {
        return Ok(json!({
            "backend": "none",
            "fallback_used": false,
            "provider_attempts": [],
            "extract_summary": {
                "max_extract_chars_per_page": ctx.max_extract_chars,
                "page_count": 0,
                "truncated_page_count": 0,
                "total_original_chars": 0,
                "total_returned_chars": 0,
                "total_omitted_chars": 0
            },
            "results": []
        }));
    }

    let outcome = extract_with_fallback(&ctx.client, &normalized, ctx.max_extract_chars).await?;
    let page_count = outcome.pages.len();
    let truncated_page_count = outcome.pages.iter().filter(|page| page.truncated).count();
    let total_original_chars: usize = outcome
        .pages
        .iter()
        .map(|page| page.original_content_chars)
        .sum();
    let total_returned_chars: usize = outcome.pages.iter().map(|page| page.content_chars).sum();
    let total_omitted_chars = total_original_chars.saturating_sub(total_returned_chars);

    Ok(json!({
        "backend": outcome.backend,
        "fallback_used": outcome.fallback_used,
        "provider_attempts": outcome.attempts,
        "extract_summary": {
            "max_extract_chars_per_page": ctx.max_extract_chars,
            "page_count": page_count,
            "truncated_page_count": truncated_page_count,
            "total_original_chars": total_original_chars,
            "total_returned_chars": total_returned_chars,
            "total_omitted_chars": total_omitted_chars
        },
        "results": outcome.pages
    }))
}

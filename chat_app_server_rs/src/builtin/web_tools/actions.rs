use serde_json::{json, Value};

use super::provider::{firecrawl_extract, firecrawl_search};
use super::BoundContext;

pub(super) async fn web_search_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
) -> Result<Value, String> {
    let search_limit = limit
        .unwrap_or(ctx.default_search_limit)
        .clamp(1, ctx.max_search_limit);
    let web = firecrawl_search(&ctx.client, query.as_str(), search_limit).await?;
    Ok(json!({
        "success": true,
        "backend": "firecrawl",
        "data": {
            "web": web
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
            "results": []
        }));
    }

    let pages = firecrawl_extract(&ctx.client, &normalized, ctx.max_extract_chars).await?;
    Ok(json!({
        "results": pages
    }))
}

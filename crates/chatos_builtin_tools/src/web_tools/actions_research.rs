use serde_json::{json, Value};

use super::actions_shared::{
    build_extract_results_brief, build_search_results_brief, normalize_inline_text,
};
use super::BoundContext;
use crate::research_findings::{
    push_unique_text, response_research_warning, response_source_highlights,
    top_extract_source_titles, top_search_hit_titles,
};
use crate::research_output::{
    build_extract_summary_line, build_search_summary_line, ExtractStatusStyle,
    ExtractSummaryLineOptions, SearchSummaryLineOptions,
};
use crate::research_payloads::{
    build_empty_extract_payload, build_empty_search_payload, build_extract_payload_from_research,
    build_search_payload_from_outcome,
};
use crate::research_summary::{
    apply_research_execution_summary, build_empty_research_summary, set_research_summary_warning,
};
use crate::research_summary_view::research_summary_view;
use crate::web_tools::provider::{
    run_research_with_fallback, BrowserRenderOptions, ExtractedPage, SearchHit,
};

pub(super) async fn web_research_with_context(
    ctx: BoundContext,
    query: String,
    limit: Option<usize>,
    extract_top: Option<usize>,
) -> Result<Value, String> {
    let search_limit = limit
        .unwrap_or(ctx.default_search_limit)
        .clamp(1, ctx.max_search_limit);
    let desired_extract_count = extract_top
        .unwrap_or(search_limit.min(3))
        .min(ctx.max_extract_urls);
    let research = match run_research_with_fallback(
        &ctx.client,
        query.as_str(),
        search_limit,
        desired_extract_count,
        ctx.max_extract_urls,
        ctx.max_extract_chars,
        Some(&BrowserRenderOptions {
            workspace_dir: ctx.workspace_dir.clone(),
            command_timeout_seconds: ctx.browser_command_timeout_seconds,
        }),
    )
    .await
    {
        Ok(execution) => execution,
        Err(err) => {
            let research_warning = err.clone();
            let mut research_summary = build_empty_research_summary(None);
            set_research_summary_warning(&mut research_summary, Some(err.as_str()));
            let mut response = json!({
                "_summary_text": format!(
                    "Web research for \"{}\" could not reach any search provider, so no external sources were gathered. {}",
                    normalize_inline_text(query.as_str(), 160),
                    normalize_inline_text(err.as_str(), 220),
                ),
                "success": true,
                "status": "degraded",
                "degraded": true,
                "warning": err,
                "query": query,
                "selected_urls": [],
                "research_warning": research_warning,
                "search_available": false,
                "research_summary": research_summary,
                "search": build_empty_search_payload(),
                "extract": build_empty_extract_payload(ctx.max_extract_chars)
            });
            response["research_findings"] = build_web_research_findings(&response);
            return Ok(response);
        }
    };
    let mut research_summary = build_empty_research_summary(None);
    apply_research_execution_summary(
        &mut research_summary,
        &research.search,
        research.selected_urls.len(),
        &research.extract,
    );
    set_research_summary_warning(&mut research_summary, research.extract.warning.as_deref());
    let summary_text = build_research_summary(
        query.as_str(),
        research.search.hits.as_slice(),
        research.extract.pages.as_slice(),
        research.selected_urls.len(),
        research.extract.stats.truncated_page_count,
        research.extract.stats.total_omitted_chars,
        research.extract.warning.as_deref(),
    );

    let mut response = json!({
        "_summary_text": summary_text,
        "success": true,
        "query": query,
        "selected_urls": research.selected_urls,
        "research_summary": research_summary,
        "search": build_search_payload_from_outcome(
            &research.search,
            build_search_results_brief(research.search.hits.as_slice())
        ),
        "extract": build_extract_payload_from_research(
            &research.extract,
            build_extract_results_brief(research.extract.pages.as_slice()),
            ctx.max_extract_chars
        )
    });
    response["research_findings"] = build_web_research_findings(&response);
    Ok(response)
}

pub(super) fn build_research_summary(
    query: &str,
    hits: &[SearchHit],
    pages: &[ExtractedPage],
    selected_url_count: usize,
    truncated_page_count: usize,
    total_omitted_chars: usize,
    extract_warning: Option<&str>,
) -> String {
    let mut lines = vec![format!(
        "Web research for \"{}\" found {} result(s) and extracted {} page(s) (selected URLs: {}, truncated pages: {}, omitted chars: {}).",
        normalize_inline_text(query, 160),
        hits.len(),
        pages.len(),
        selected_url_count,
        truncated_page_count,
        total_omitted_chars,
    )];

    if hits.is_empty() {
        lines.push("No search hits were returned.".to_string());
    } else {
        for (index, hit) in hits.iter().take(3).enumerate() {
            lines.push(build_search_summary_line(
                index,
                hit,
                SearchSummaryLineOptions {
                    label_prefix: Some("search"),
                    fallback_title_to_url: true,
                    title_max_chars: 120,
                    url_max_chars: 140,
                    description_max_chars: 160,
                },
            ));
        }
    }

    if pages.is_empty() {
        if selected_url_count > 0 {
            lines.push("No pages were extracted.".to_string());
        }
    } else {
        for (index, page) in pages.iter().take(3).enumerate() {
            lines.push(build_extract_summary_line(
                index,
                page,
                ExtractSummaryLineOptions {
                    label_prefix: Some("extract"),
                    fallback_title_to_url: true,
                    title_max_chars: 120,
                    url_max_chars: 140,
                    status_style: ExtractStatusStyle::HumanReadable,
                    status_error_max_chars: 120,
                    ok_preview_max_chars: 180,
                    error_preview_max_chars: 160,
                },
            ));
        }
    }

    if let Some(warning) = extract_warning
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!(
            "Research warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }

    lines.join("\n")
}

pub(super) fn build_web_research_findings(response: &Value) -> Value {
    let query = response
        .get("query")
        .and_then(|value| value.as_str())
        .map(|value| normalize_inline_text(value, 160))
        .filter(|value| !value.is_empty());
    let summary = research_summary_view(response);
    let answer_frame = format!(
        "Web research{} found {} result(s) and extracted {} page(s).",
        query
            .as_deref()
            .map(|value| format!(" for \"{}\"", value))
            .unwrap_or_default(),
        summary.search_count,
        summary.extract_count,
    );

    let mut web_findings = Vec::new();
    push_unique_text(
        &mut web_findings,
        format!("Search returned {} result(s).", summary.search_count),
    );
    let search_titles = top_search_hit_titles(response, 100, 3);
    if !search_titles.is_empty() {
        push_unique_text(
            &mut web_findings,
            format!("Top search hits: {}.", search_titles.join(" | ")),
        );
    }
    push_unique_text(
            &mut web_findings,
            format!(
                "Extraction reviewed {} selected URL(s) and returned {} page(s); truncated pages: {}, omitted chars: {}.",
            summary.selected_url_count,
            summary.extract_count,
            summary.truncated_page_count,
            summary.total_omitted_chars,
        ),
    );
    let extracted_titles = top_extract_source_titles(response, 90, 60, 3);
    if !extracted_titles.is_empty() {
        push_unique_text(
            &mut web_findings,
            format!("Key extracted sources: {}.", extracted_titles.join(" | ")),
        );
    }
    if let Some(warning) = response_research_warning(response) {
        push_unique_text(
            &mut web_findings,
            format!("Research warning: {}.", normalize_inline_text(warning, 180)),
        );
    }

    let source_highlights = response_source_highlights(response);

    let mut recommended_next_steps = Vec::new();
    if summary.search_count == 0 {
        push_unique_text(
            &mut recommended_next_steps,
            "Refine the query with specific product, site, or date terms so web_search can surface more relevant hits.".to_string(),
        );
    } else if summary.extract_count == 0 {
        push_unique_text(
            &mut recommended_next_steps,
            "Use web_extract on selected_urls or increase extract_top when you need source text instead of search snippets.".to_string(),
        );
    }
    if summary.truncated_page_count > 0 {
        push_unique_text(
            &mut recommended_next_steps,
            "Re-run extraction on the most relevant URL if one source needs deeper reading with less truncation.".to_string(),
        );
    }
    if summary.warning_present {
        push_unique_text(
            &mut recommended_next_steps,
            "Check the warning field because this research bundle used a degraded extraction path."
                .to_string(),
        );
    }
    push_unique_text(
        &mut recommended_next_steps,
        "If the question depends on the page currently open in the browser, prefer browser_research so page context and web sources are combined in one run.".to_string(),
    );

    json!({
        "answer_frame": answer_frame,
        "page_findings": [],
        "web_findings": web_findings,
        "source_highlights": source_highlights,
        "recommended_next_steps": recommended_next_steps,
    })
}

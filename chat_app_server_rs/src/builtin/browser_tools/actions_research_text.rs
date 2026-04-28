use serde_json::{json, Value};

use crate::builtin::browser_page_insights::{
    inspection_warning_line, latest_console_text_line, latest_js_error_line,
    page_label_from_response, page_state_warning_line,
};
use crate::builtin::browser_page_state_view::{
    browser_console_state_view, browser_inspection_steps_view,
};
use crate::builtin::research_findings::{
    push_unique_text, response_research_warning, response_source_highlights,
    top_extract_source_titles, top_search_hit_titles,
};
use crate::builtin::research_summary_view::research_summary_view;
use super::actions_shared::normalize_inline_text;

pub(super) fn build_browser_research_summary(response: &Value) -> String {
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

    if let Some(steps) = browser_inspection_steps_view(page) {
        parts.push(format!(
            "Page inspect steps: snapshot={}, console={}, vision={}.",
            steps.snapshot, steps.console, steps.vision
        ));
    }

    let include_web = response
        .get("include_web")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let summary = research_summary_view(response);
    if include_web {
        let query = response
            .get("web_query")
            .and_then(|value| value.as_str())
            .map(|value| normalize_inline_text(value, 160))
            .filter(|value| !value.is_empty());

        match query {
            Some(query) => parts.push(format!(
                "Supplemental web research for \"{}\" returned {} result(s) and extracted {} page(s) (selected URLs: {}, omitted chars: {}).",
                query,
                summary.search_count,
                summary.extract_count,
                summary.selected_url_count,
                summary.total_omitted_chars,
            )),
            None => parts.push(format!(
                "Supplemental web research returned {} result(s) and extracted {} page(s) (selected URLs: {}, omitted chars: {}).",
                summary.search_count,
                summary.extract_count,
                summary.selected_url_count,
                summary.total_omitted_chars,
            )),
        }
    } else {
        parts.push("External web research was skipped.".to_string());
    }

    if let Some(warning) = response_research_warning(response) {
        parts.push(format!(
            "Research warning: {}.",
            normalize_inline_text(warning, 180)
        ));
    }

    parts.join(" ")
}

pub(super) fn build_browser_research_findings(response: &Value) -> Value {
    let page = response.get("page").unwrap_or(response);
    let include_web = response
        .get("include_web")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let summary = research_summary_view(response);
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
    let page_success = summary
        .page_success
        .or_else(|| page.get("success").and_then(|value| value.as_bool()))
        .unwrap_or(false);
    let console = browser_console_state_view(page);
    let total_messages = console.total_messages;
    let total_errors = console.total_errors;

    let answer_frame = if include_web {
        let focus = question
            .as_deref()
            .map(|value| format!(" for \"{}\"", value))
            .unwrap_or_default();
        format!(
            "Combined page and web research{} {} Page inspect: {}. External search found {} result(s) and extracted {} page(s).",
            focus,
            if page_success {
                "completed."
            } else {
                "completed with a partial page signal."
            },
            if page_success { "usable" } else { "degraded" },
            summary.search_count,
            summary.extract_count,
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
    if let Some(steps) = browser_inspection_steps_view(page) {
        push_unique_text(
            &mut page_findings,
            format!(
                "Inspection steps finished with snapshot={}, console={}, vision={}.",
                steps.snapshot, steps.console, steps.vision
            ),
        );
    }
    if total_messages > 0 || total_errors > 0 || console.has_message_count_by_type {
        push_unique_text(
            &mut page_findings,
            format!(
                "Console inspection captured {} message(s) and {} JavaScript error(s).",
                total_messages, total_errors
            ),
        );
    }
    if let Some(line) = latest_js_error_line(page, "Latest JS error") {
        push_unique_text(&mut page_findings, line);
    }
    if let Some(line) = latest_console_text_line(page, "Latest console note") {
        push_unique_text(&mut page_findings, line);
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
    if let Some(line) = inspection_warning_line(page) {
        push_unique_text(&mut page_findings, line);
    }
    if let Some(line) = page_state_warning_line(page) {
        push_unique_text(&mut page_findings, line);
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
                search_focus, summary.search_count
            ),
        );
        let search_titles = top_search_hit_titles(response, 100, 3);
        if !search_titles.is_empty() {
            push_unique_text(
                &mut web_findings,
                format!("Top search hits: {}.", search_titles.join(" | ")),
            );
        }
        if summary.extract_count > 0
            || summary.selected_url_count > 0
            || !matches!(summary.extract_backend.as_str(), "none")
        {
            push_unique_text(
                &mut web_findings,
                format!(
                    "Source extraction reviewed {} selected URL(s) and returned {} page(s); truncated pages: {}, omitted chars: {}.",
                    summary.selected_url_count,
                    summary.extract_count,
                    summary.truncated_page_count,
                    summary.total_omitted_chars,
                ),
            );
        }
        let extracted_titles = top_extract_source_titles(response, 90, 60, 3);
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
    if let Some(warning) = response_research_warning(response) {
        push_unique_text(
            &mut web_findings,
            format!("Research warning: {}.", normalize_inline_text(warning, 180)),
        );
    }

    let source_highlights = response_source_highlights(response);

    let mut recommended_next_steps = Vec::new();
    if !page_success {
        push_unique_text(
            &mut recommended_next_steps,
            "Refresh or reopen the current page, then rerun browser_research to recover full page context.".to_string(),
        );
    }
    if include_web {
        if summary.search_count == 0 {
            push_unique_text(
                &mut recommended_next_steps,
                "Tighten web_query with product, site, or date keywords so web_search can return more specific hits.".to_string(),
            );
        } else if summary.extract_count == 0 {
            push_unique_text(
                &mut recommended_next_steps,
                "Run web_extract on selected_urls or increase extract_top when you need the source text, not just hit titles.".to_string(),
            );
        }
        if summary.truncated_page_count > 0 {
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
    if summary.warning_present || response_research_warning(response).is_some() {
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

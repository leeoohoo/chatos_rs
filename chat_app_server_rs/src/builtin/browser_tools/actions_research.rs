use serde_json::{json, Value};

use crate::builtin::research_payloads::build_empty_search_payload;
use crate::builtin::research_summary::{
    apply_research_execution_summary, build_empty_research_summary,
    set_research_summary_warning,
};
use crate::builtin::web_tools::provider::{
    build_web_client, run_research_with_fallback, BrowserRenderOptions,
};

use super::actions_research_payloads::{
    browser_research_empty_extract_payload, browser_research_extract_payload,
    browser_research_search_payload,
};
use super::actions_research_text::{
    build_browser_research_findings, build_browser_research_summary,
};
use super::actions_shared::{
    summarize_browser_failure,
};
use super::{
    browser_inspect_with_context, BoundContext, DEFAULT_BROWSER_RESEARCH_LIMIT,
    DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS,
    DEFAULT_BROWSER_RESEARCH_REQUEST_TIMEOUT_SECONDS, MAX_BROWSER_RESEARCH_EXTRACT_URLS,
    MAX_BROWSER_RESEARCH_LIMIT,
};

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
    let mut research_summary = build_empty_research_summary(Some(page_success));

    if include_web {
        let query = web_query
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| question.clone());
        let search_limit = web_limit
            .unwrap_or(DEFAULT_BROWSER_RESEARCH_LIMIT)
            .clamp(1, MAX_BROWSER_RESEARCH_LIMIT);
        let desired_extract_count = extract_top
            .unwrap_or(search_limit.min(3))
            .min(MAX_BROWSER_RESEARCH_EXTRACT_URLS);
        response["web_query"] = Value::String(query.clone());

        match build_web_client(
            std::time::Duration::from_secs(
                DEFAULT_BROWSER_RESEARCH_REQUEST_TIMEOUT_SECONDS,
            ),
            "chatos-rs-browser-research/0.1",
        ) {
            Ok(client) => match run_research_with_fallback(
                &client,
                query.as_str(),
                search_limit,
                desired_extract_count,
                MAX_BROWSER_RESEARCH_EXTRACT_URLS,
                DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS,
                Some(&BrowserRenderOptions {
                    workspace_dir: ctx.workspace_dir.clone(),
                    command_timeout_seconds: ctx.command_timeout_seconds,
                }),
            )
            .await
            {
                Ok(research) => {
                    selected_urls = research.selected_urls.clone();
                    apply_research_execution_summary(
                        &mut research_summary,
                        &research.search,
                        selected_urls.len(),
                        &research.extract,
                    );
                    if let Some(warning) = research
                        .extract
                        .warning
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                    {
                        warnings.push(format!("web_extract: {}", warning));
                    }
                    response["search"] = browser_research_search_payload(&research);
                    response["extract"] = browser_research_extract_payload(&research);
                }
                Err(err) => {
                    warnings.push(format!("web_search: {}", err));
                    response["search"] = build_empty_search_payload();
                    response["extract"] = browser_research_empty_extract_payload();
                }
            },
            Err(err) => {
                warnings.push(format!(
                    "web_search: build browser_research client failed: {}",
                    err
                ));
                response["search"] = build_empty_search_payload();
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
    set_research_summary_warning(
        &mut research_summary,
        response
        .get("research_warning")
        .and_then(|value| value.as_str()),
    );
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

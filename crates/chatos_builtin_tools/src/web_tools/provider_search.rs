// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::Value;
use tokio::time::{sleep, Duration};

use crate::browser_runtime::browser_backend_available;

const BING_SEARCH_ENDPOINT: &str = "https://cn.bing.com/search";
const DUCKDUCKGO_HTML_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const DUCKDUCKGO_BROWSER_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const DUCKDUCKGO_INSTANT_ENDPOINT: &str = "https://api.duckduckgo.com/";
const SEARCH_PAGE_DELAY_MS: u64 = 350;
const BROWSER_SEARCH_OPEN_TIMEOUT_SECONDS: u64 = 8;
const BROWSER_SEARCH_COMMAND_TIMEOUT_SECONDS: u64 = 6;
const BROWSER_SEARCH_SETTLE_DELAY_MS: u64 = 600;

use self::super::provider_browser_support::{
    browser_command_error, close_browser_page, eval_on_browser_page, is_browser_command_success,
    open_browser_page, parse_browser_eval_result,
};
use self::super::provider_search_support::{
    browser_search_eval_expression, extract_browser_search_hits, extract_duckduckgo_next_page_form,
    looks_like_duckduckgo_antibot, parse_bing_html_results as parse_bing_html_results_support,
    parse_duckduckgo_html_results as parse_duckduckgo_html_results_support,
    parse_duckduckgo_instant_answer_response,
};
use self::super::provider_utils::{
    read_success_response_text, summarize_reqwest_error, DEFAULT_USER_AGENT,
};
use super::{BrowserRenderOptions, SearchHit};

pub(super) async fn duckduckgo_html_search(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let mut next_form: Option<Vec<(String, String)>> = None;
    let mut is_first_page = true;

    while results.len() < limit {
        let request = if is_first_page {
            client.get(DUCKDUCKGO_HTML_ENDPOINT).query(&[
                ("q", query),
                ("kp", "-1"),
                ("kl", "wt-wt"),
            ])
        } else {
            let params = next_form
                .as_ref()
                .ok_or_else(|| "next page form data missing".to_string())?;
            client.post(DUCKDUCKGO_HTML_ENDPOINT).form(params)
        };

        let response = request
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .send()
            .await
            .map_err(|err| summarize_reqwest_error(&err, "search request failed"))?;

        let body = read_success_response_text(response, "read response failed", 800).await?;

        if looks_like_duckduckgo_antibot(&body) {
            return Err("duckduckgo returned an anti-bot challenge page".to_string());
        }

        let page_hits = parse_duckduckgo_html_results(&body, &mut seen);
        if page_hits.is_empty() {
            break;
        }

        results.extend(page_hits.into_iter().take(limit - results.len()));
        if results.len() >= limit {
            break;
        }

        next_form = extract_duckduckgo_next_page_form(&body);
        if next_form.is_none() {
            break;
        }

        is_first_page = false;
        sleep(Duration::from_millis(SEARCH_PAGE_DELAY_MS)).await;
    }

    Ok(results)
}

pub(super) fn parse_duckduckgo_html_results(
    body: &str,
    seen: &mut HashSet<String>,
) -> Vec<SearchHit> {
    parse_duckduckgo_html_results_support(body, seen)
}

pub(super) async fn bing_html_search(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let count = limit.max(10).min(50).to_string();
    let response = client
        .get(BING_SEARCH_ENDPOINT)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .query(&[("q", query), ("count", count.as_str())])
        .send()
        .await
        .map_err(|err| summarize_reqwest_error(&err, "bing search request failed"))?;

    let body = read_success_response_text(response, "read response failed", 800).await?;

    Ok(parse_bing_html_results(&body, limit))
}

pub(super) fn parse_bing_html_results(body: &str, limit: usize) -> Vec<SearchHit> {
    parse_bing_html_results_support(body, limit)
}
pub(super) async fn duckduckgo_instant_answer_search(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let response = client
        .get(DUCKDUCKGO_INSTANT_ENDPOINT)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .query(&[
            ("q", query),
            ("format", "json"),
            ("no_redirect", "1"),
            ("no_html", "1"),
            ("skip_disambig", "1"),
        ])
        .send()
        .await
        .map_err(|err| summarize_reqwest_error(&err, "instant answer request failed"))?;

    let text = read_success_response_text(response, "read response failed", 800).await?;

    parse_duckduckgo_instant_answer_response(&text, limit)
}

pub(super) async fn duckduckgo_browser_search(
    query: &str,
    limit: usize,
    options: &BrowserRenderOptions,
) -> Result<Vec<SearchHit>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    if let Err(err) = browser_backend_available() {
        return Err(format!("browser search unavailable: {}", err));
    }

    let search_url = format!(
        "{}?q={}&kl=wt-wt&kp=-1",
        DUCKDUCKGO_BROWSER_ENDPOINT,
        urlencoding::encode(query)
    );
    let open_timeout = options
        .command_timeout_seconds
        .clamp(3, BROWSER_SEARCH_OPEN_TIMEOUT_SECONDS);
    let command_timeout = options
        .command_timeout_seconds
        .clamp(3, BROWSER_SEARCH_COMMAND_TIMEOUT_SECONDS);
    let Some(session) = open_browser_page(search_url.as_str(), options, open_timeout).await? else {
        return Err(browser_command_error(
            &Value::Null,
            "browser search page failed to open",
        ));
    };

    sleep(Duration::from_millis(BROWSER_SEARCH_SETTLE_DELAY_MS)).await;

    let eval_result = eval_on_browser_page(
        &session,
        browser_search_eval_expression(limit),
        options,
        command_timeout,
    )
    .await?;
    close_browser_page(&session, options).await;

    if !is_browser_command_success(&eval_result) {
        return Err(browser_command_error(
            &eval_result,
            "browser search result inspection failed",
        ));
    }

    let parsed = eval_result
        .get("data")
        .and_then(|value| value.get("result"))
        .cloned()
        .map(parse_browser_eval_result)
        .unwrap_or(Value::Null);

    extract_browser_search_hits(&parsed, limit)
}

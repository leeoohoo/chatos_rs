use super::provider_extract::native_extract;
use super::provider_fallback::{run_timed_strategy, StrategyRun};
use super::provider_search::{
    bing_html_search, duckduckgo_browser_search, duckduckgo_html_search,
    duckduckgo_instant_answer_search,
};
use super::provider_types::{
    BrowserRenderOptions, ExtractOutcome, ExtractedPage, ProviderAttempt, SearchHit, SearchOutcome,
};
use super::provider_utils::summarize_attempts;

const NATIVE_SEARCH_BACKEND: &str = "chatos_native_search";
const NATIVE_EXTRACT_BACKEND: &str = "chatos_native_extract";
const SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS: u64 = 8;
const SEARCH_BROWSER_STRATEGY_TIMEOUT_SECONDS: u64 = 12;

pub(crate) async fn search_with_fallback(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> Result<SearchOutcome, String> {
    let mut attempts = Vec::new();
    let mut html_search_succeeded = false;

    let (html_result, instant_result) = tokio::join!(
        run_timed_strategy(
            "duckduckgo_html",
            SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS,
            duckduckgo_html_search(client, query, limit),
        ),
        run_timed_strategy(
            "duckduckgo_instant_answer",
            SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS,
            duckduckgo_instant_answer_search(client, query, limit),
        ),
    );

    match html_result {
        StrategyRun::Success(hits) if !hits.is_empty() => {
            return Ok(build_search_outcome(false, attempts, hits));
        }
        StrategyRun::Success(_) => {
            html_search_succeeded = true;
        }
        StrategyRun::Failed(attempt) => attempts.push(attempt),
    }

    match instant_result {
        StrategyRun::Success(hits) if !hits.is_empty() => {
            return Ok(build_search_outcome(true, attempts, hits));
        }
        StrategyRun::Success(_) => {}
        StrategyRun::Failed(attempt) => attempts.push(attempt),
    }

    match run_timed_strategy(
        "bing_html",
        SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS,
        bing_html_search(client, query, limit),
    )
    .await
    {
        StrategyRun::Success(hits) if !hits.is_empty() => {
            return Ok(build_search_outcome(true, attempts, hits));
        }
        StrategyRun::Success(_) => {}
        StrategyRun::Failed(attempt) => attempts.push(attempt),
    }

    if !html_search_succeeded {
        if let Some(options) = browser_options.filter(|_| limit > 0) {
            match run_timed_strategy(
                "duckduckgo_browser",
                SEARCH_BROWSER_STRATEGY_TIMEOUT_SECONDS,
                duckduckgo_browser_search(query, limit, options),
            )
            .await
            {
                StrategyRun::Success(hits) if !hits.is_empty() => {
                    return Ok(build_search_outcome(true, attempts, hits));
                }
                StrategyRun::Success(_) => {}
                StrategyRun::Failed(attempt) => attempts.push(attempt),
            }
        }
    }

    if html_search_succeeded {
        return Ok(build_search_outcome(false, attempts, Vec::new()));
    }

    Err(format!(
        "web_search failed across internal strategies: {}",
        summarize_attempts(&attempts)
    ))
}

pub(crate) async fn extract_with_fallback(
    client: &reqwest::Client,
    urls: &[String],
    max_extract_chars: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> Result<ExtractOutcome, String> {
    let pages = native_extract(client, urls, max_extract_chars, browser_options).await?;
    Ok(build_extract_outcome(pages))
}

fn build_search_outcome(
    fallback_used: bool,
    attempts: Vec<ProviderAttempt>,
    hits: Vec<SearchHit>,
) -> SearchOutcome {
    SearchOutcome {
        backend: NATIVE_SEARCH_BACKEND.to_string(),
        fallback_used,
        attempts,
        hits,
    }
}

fn build_extract_outcome(pages: Vec<ExtractedPage>) -> ExtractOutcome {
    ExtractOutcome {
        backend: NATIVE_EXTRACT_BACKEND.to_string(),
        fallback_used: false,
        attempts: Vec::new(),
        pages,
    }
}

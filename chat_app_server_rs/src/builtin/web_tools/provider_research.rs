use reqwest::Client;

use super::provider_types::{
    BrowserRenderOptions, ExtractOutcome, ExtractedPage, ProviderAttempt, SearchOutcome,
};
use super::provider_research_support::select_research_extract_urls;
use super::{extract_with_fallback, search_with_fallback};

#[derive(Debug, Clone)]
pub(crate) struct ResearchExecution {
    pub search: SearchOutcome,
    pub selected_urls: Vec<String>,
    pub extract: ResearchExtractExecution,
}

#[derive(Debug, Clone)]
pub(crate) struct ResearchExtractExecution {
    pub backend: String,
    pub fallback_used: bool,
    pub attempts: Vec<ProviderAttempt>,
    pub pages: Vec<ExtractedPage>,
    pub warning: Option<String>,
    pub stats: ResearchExtractStats,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ResearchExtractStats {
    pub page_count: usize,
    pub truncated_page_count: usize,
    pub total_original_chars: usize,
    pub total_returned_chars: usize,
    pub total_omitted_chars: usize,
}

pub(crate) async fn run_research_with_fallback(
    client: &Client,
    query: &str,
    search_limit: usize,
    desired_extract_count: usize,
    max_extract_urls: usize,
    max_extract_chars: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> Result<ResearchExecution, String> {
    let search = search_with_fallback(client, query, search_limit, browser_options).await?;
    let selected_urls = select_research_extract_urls(
        search.hits.as_slice(),
        desired_extract_count.min(max_extract_urls),
        max_extract_urls,
    );
    let extract = run_extract_with_fallback(
        client,
        selected_urls.as_slice(),
        !search.hits.is_empty(),
        desired_extract_count.min(max_extract_urls),
        max_extract_chars,
        browser_options,
    )
    .await;

    Ok(ResearchExecution {
        search,
        selected_urls,
        extract,
    })
}

async fn run_extract_with_fallback(
    client: &Client,
    selected_urls: &[String],
    search_had_hits: bool,
    desired_extract_count: usize,
    max_extract_chars: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> ResearchExtractExecution {
    if selected_urls.is_empty() {
        return empty_extract_execution(extract_skip_warning(
            desired_extract_count,
            search_had_hits,
        ));
    }

    match extract_with_fallback(client, selected_urls, max_extract_chars, browser_options).await {
        Ok(outcome) => extract_execution_from_outcome(outcome),
        Err(err) => empty_extract_execution(Some(err)),
    }
}

fn extract_execution_from_outcome(outcome: ExtractOutcome) -> ResearchExtractExecution {
    let stats = compute_research_extract_stats(outcome.pages.as_slice());
    ResearchExtractExecution {
        backend: outcome.backend,
        fallback_used: outcome.fallback_used,
        attempts: outcome.attempts,
        pages: outcome.pages,
        warning: None,
        stats,
    }
}

fn empty_extract_execution(warning: Option<String>) -> ResearchExtractExecution {
    ResearchExtractExecution {
        backend: "none".to_string(),
        fallback_used: false,
        attempts: Vec::new(),
        pages: Vec::new(),
        warning,
        stats: ResearchExtractStats::default(),
    }
}

fn extract_skip_warning(desired_extract_count: usize, search_had_hits: bool) -> Option<String> {
    if desired_extract_count == 0 {
        return Some("Extraction was skipped because extract_top was set to 0.".to_string());
    }
    if !search_had_hits {
        return Some("No search hits were returned, so nothing was extracted.".to_string());
    }
    None
}

pub(crate) fn compute_research_extract_stats(pages: &[ExtractedPage]) -> ResearchExtractStats {
    let page_count = pages.len();
    let truncated_page_count = pages.iter().filter(|page| page.truncated).count();
    let total_original_chars: usize = pages.iter().map(|page| page.original_content_chars).sum();
    let total_returned_chars: usize = pages.iter().map(|page| page.content_chars).sum();
    ResearchExtractStats {
        page_count,
        truncated_page_count,
        total_original_chars,
        total_returned_chars,
        total_omitted_chars: total_original_chars.saturating_sub(total_returned_chars),
    }
}

#[cfg(test)]
mod tests {
    use super::compute_research_extract_stats;
    use super::super::provider_types::ExtractedPage;

    #[test]
    fn compute_extract_stats_aggregates_counts_and_omitted_chars() {
        let stats = compute_research_extract_stats(&[
            ExtractedPage {
                url: "https://example.com/a".to_string(),
                title: "A".to_string(),
                content: "alpha".to_string(),
                content_chars: 5,
                original_content_chars: 7,
                truncated: true,
                content_summary: None,
                error: None,
            },
            ExtractedPage {
                url: "https://example.com/b".to_string(),
                title: "B".to_string(),
                content: "beta".to_string(),
                content_chars: 4,
                original_content_chars: 4,
                truncated: false,
                content_summary: None,
                error: None,
            },
        ]);

        assert_eq!(stats.page_count, 2);
        assert_eq!(stats.truncated_page_count, 1);
        assert_eq!(stats.total_original_chars, 11);
        assert_eq!(stats.total_returned_chars, 9);
        assert_eq!(stats.total_omitted_chars, 2);
    }
}

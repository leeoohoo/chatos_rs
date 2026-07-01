// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "provider_browser_support.rs"]
mod provider_browser_support;
#[path = "provider_extract.rs"]
mod provider_extract;
#[path = "provider_fallback.rs"]
mod provider_fallback;
#[path = "provider_flow.rs"]
mod provider_flow;
#[path = "provider_research.rs"]
mod provider_research;
#[path = "provider_research_support.rs"]
mod provider_research_support;
#[path = "provider_search.rs"]
mod provider_search;
#[path = "provider_search_shared.rs"]
mod provider_search_shared;
#[path = "provider_search_support.rs"]
mod provider_search_support;
#[path = "provider_types.rs"]
mod provider_types;
#[path = "provider_url_policy.rs"]
mod provider_url_policy;
#[path = "provider_utils.rs"]
mod provider_utils;

pub use self::provider_flow::{extract_with_fallback, search_with_fallback};
pub use self::provider_research::{
    compute_research_extract_stats, run_research_with_fallback, ResearchExecution,
    ResearchExtractExecution, ResearchExtractStats,
};
pub use self::provider_types::{
    BrowserRenderOptions, ExtractOutcome, ExtractedPage, ProviderAttempt, SearchHit, SearchOutcome,
};
pub use self::provider_url_policy::{build_web_client, normalize_public_web_url};

#[cfg(test)]
mod tests {
    use super::provider_search::{parse_bing_html_results, parse_duckduckgo_html_results};
    use super::provider_search_support::extract_browser_search_hits;
    use super::provider_utils::sanitize_provider_error;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn duckduckgo_html_results_decode_redirects_and_dedupe_urls() {
        let html = r#"
            <html>
              <body>
                <div class="result web-result">
                  <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fone">Example One</a>
                  <div class="result__snippet">First result snippet.</div>
                </div>
                <div class="result web-result">
                  <a class="result__a" href="https://example.com/one">Example One Duplicate</a>
                  <div class="result__snippet">Duplicate should be removed.</div>
                </div>
                <div class="result web-result">
                  <a class="result__a" href="https://example.com/two">Example Two</a>
                  <div class="result__snippet">Second result snippet.</div>
                </div>
              </body>
            </html>
        "#;

        let mut seen = HashSet::new();
        let hits = parse_duckduckgo_html_results(html, &mut seen);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://example.com/one");
        assert_eq!(hits[0].title, "Example One");
        assert_eq!(hits[1].url, "https://example.com/two");
    }

    #[test]
    fn duckduckgo_html_results_drop_private_redirect_targets() {
        let html = r#"
            <html>
              <body>
                <div class="result web-result">
                  <a class="result__a" href="https://duckduckgo.com/l/?uddg=http%3A%2F%2F127.0.0.1%3A8080%2Fadmin">Blocked</a>
                  <div class="result__snippet">Should be filtered.</div>
                </div>
                <div class="result web-result">
                  <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fsafe">Allowed</a>
                  <div class="result__snippet">Safe result.</div>
                </div>
              </body>
            </html>
        "#;

        let mut seen = HashSet::new();
        let hits = parse_duckduckgo_html_results(html, &mut seen);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/safe");
        assert_eq!(hits[0].title, "Allowed");
    }

    #[test]
    fn bing_html_results_extract_titles_urls_and_snippets() {
        let html = r#"
            <html>
              <body>
                <ol id="b_results">
                  <li class="b_algo">
                    <h2><a href="https://example.com/news/story">Example Story</a></h2>
                    <div class="b_caption">
                      <p>Latest breaking coverage from Example News.</p>
                    </div>
                  </li>
                  <li class="b_algo">
                    <h2><a href="https://example.com/news/story">Duplicate Story</a></h2>
                    <div class="b_caption">
                      <p>Should be removed because the URL is duplicated.</p>
                    </div>
                  </li>
                  <li class="b_algo">
                    <h2><a href="https://another.example.com/posts/update">Another Story</a></h2>
                    <div class="b_caption">
                      <p>Another source snippet.</p>
                    </div>
                  </li>
                </ol>
              </body>
            </html>
        "#;

        let hits = parse_bing_html_results(html, 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://example.com/news/story");
        assert_eq!(hits[0].title, "Example Story");
        assert!(hits[0].description.contains("Latest breaking coverage"));
        assert_eq!(hits[1].url, "https://another.example.com/posts/update");
    }

    #[test]
    fn bing_html_results_drop_private_targets() {
        let html = r#"
            <html>
              <body>
                <ol id="b_results">
                  <li class="b_algo">
                    <h2><a href="http://localhost:3000/private">Local Only</a></h2>
                    <div class="b_caption">
                      <p>Should be filtered.</p>
                    </div>
                  </li>
                  <li class="b_algo">
                    <h2><a href="https://example.com/public">Public Story</a></h2>
                    <div class="b_caption">
                      <p>Should remain.</p>
                    </div>
                  </li>
                </ol>
              </body>
            </html>
        "#;

        let hits = parse_bing_html_results(html, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/public");
        assert_eq!(hits[0].title, "Public Story");
    }

    #[test]
    fn browser_search_hits_drop_non_public_targets() {
        let parsed = json!({
            "anti_bot": false,
            "hits": [
                {
                    "url": "http://localhost:3000/private",
                    "title": "Local Only",
                    "description": "Should be filtered."
                },
                {
                    "url": "https://duckduckgo.com/l/?uddg=http%3A%2F%2F100.64.0.8%2Fprivate",
                    "title": "Carrier NAT",
                    "description": "Should be filtered."
                },
                {
                    "url": "http://router.home.arpa/status",
                    "title": "Home Router",
                    "description": "Should be filtered."
                },
                {
                    "url": "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fsafe",
                    "title": "Safe Result",
                    "description": "Should remain."
                }
            ]
        });

        let hits = extract_browser_search_hits(&parsed, 10).expect("browser hits");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/safe");
        assert_eq!(hits[0].title, "Safe Result");
    }

    #[test]
    fn sanitize_provider_error_removes_reqwest_url_noise() {
        let sanitized = sanitize_provider_error(
            "error sending request for url (https://html.duckduckgo.com/html/?q=foo&kp=-1)",
        );
        assert_eq!(sanitized, "network request could not be sent");
    }
}

use std::collections::HashSet;
use std::path::PathBuf;

use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::header::CONTENT_TYPE;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use tokio::time::{sleep, timeout, Duration};
use url::Url;

use crate::builtin::browser_runtime::{
    browser_backend_available, new_browser_session, run_browser_command,
};

const NATIVE_SEARCH_BACKEND: &str = "chatos_native_search";
const NATIVE_EXTRACT_BACKEND: &str = "chatos_native_extract";
const BING_SEARCH_ENDPOINT: &str = "https://cn.bing.com/search";
const DUCKDUCKGO_HTML_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const DUCKDUCKGO_BROWSER_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const DUCKDUCKGO_INSTANT_ENDPOINT: &str = "https://api.duckduckgo.com/";
const SEARCH_PAGE_DELAY_MS: u64 = 350;
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
const SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS: u64 = 8;
const SEARCH_BROWSER_STRATEGY_TIMEOUT_SECONDS: u64 = 12;
const MIN_BROWSER_RENDER_TRIGGER_CHARS: usize = 320;
const BROWSER_RENDER_OPEN_TIMEOUT_SECONDS: u64 = 60;
const BROWSER_SEARCH_OPEN_TIMEOUT_SECONDS: u64 = 8;
const BROWSER_SEARCH_COMMAND_TIMEOUT_SECONDS: u64 = 6;
const BROWSER_SEARCH_SETTLE_DELAY_MS: u64 = 600;

#[derive(Debug, Clone)]
pub(crate) struct BrowserRenderOptions {
    pub workspace_dir: PathBuf,
    pub command_timeout_seconds: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SearchHit {
    pub url: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExtractedPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub content_chars: usize,
    pub original_content_chars: usize,
    pub truncated: bool,
    pub content_summary: Option<ExtractContentSummary>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExtractSummaryChunk {
    pub index: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub preview: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExtractContentSummary {
    pub strategy: String,
    pub total_chars: usize,
    pub chunk_chars: usize,
    pub total_chunks: usize,
    pub sampled_chunks: Vec<ExtractSummaryChunk>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ProviderAttempt {
    pub provider: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SearchOutcome {
    pub backend: String,
    pub fallback_used: bool,
    pub attempts: Vec<ProviderAttempt>,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExtractOutcome {
    pub backend: String,
    pub fallback_used: bool,
    pub attempts: Vec<ProviderAttempt>,
    pub pages: Vec<ExtractedPage>,
}

#[derive(Debug, Clone)]
struct ResearchUrlCandidate {
    url: String,
    host: String,
    article_like: bool,
    section_like: bool,
}

#[derive(Debug, Default, Clone)]
struct HtmlMetadata {
    title: String,
    description: String,
}

#[derive(Debug, Default, Clone)]
struct StructuredDataContent {
    title: String,
    description: String,
    text_blocks: Vec<String>,
}

#[derive(Debug, Default, Clone)]
struct BrowserRenderedPage {
    url: String,
    title: String,
    content: String,
    meta_description: String,
    snapshot: String,
}

#[derive(Debug, Clone)]
enum ResponseContentKind {
    Html,
    Json,
    Text,
    Pdf,
    Unsupported(String),
}

static RE_SCRIPT_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("script regex"));
static RE_STYLE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("style regex"));
static RE_NOSCRIPT_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").expect("noscript regex"));
static RE_TEMPLATE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<template[^>]*>.*?</template>").expect("template regex"));
static RE_COMMENT_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<!--.*?-->").expect("comment regex"));
static RE_LAYOUT_TAG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?is)<(?:header|footer|nav|aside|dialog)[^>]*>.*?</(?:header|footer|nav|aside|dialog)>",
    )
    .expect("layout regex")
});
static RE_NOISE_BLOCK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<(?:div|section|aside|nav|header|footer|form)[^>]*(?:id|class|role|aria-label|data-testid)\s*=\s*["'][^"']*(?:header|footer|nav|sidebar|cookie|modal|popup|social|share|breadcrumb|advert|ads|banner|menu|toolbar)[^"']*["'][^>]*>.*?</(?:div|section|aside|nav|header|footer|form)>"#,
    )
    .expect("noise regex")
});
static RE_HTML_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<[^>]+>").expect("html tag regex"));
static REQWEST_URL_SUFFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\s+for url \([^)]+\)"#).expect("reqwest url suffix regex"));

static RESULT_BLOCK_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse(".result, .result.web-result").expect("result selector"));
static RESULT_LINK_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("a.result__a, a.result-link, h2 a, .links_main a")
        .expect("result link selector")
});
static RESULT_SNIPPET_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse(".result__snippet, .result-snippet, .snippet, .result__extras__body")
        .expect("result snippet selector")
});
static BING_RESULT_BLOCK_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("li.b_algo").expect("bing result selector"));
static BING_RESULT_LINK_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("h2 a").expect("bing result link selector"));
static BING_RESULT_SNIPPET_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse(".b_caption p, .b_snippet, p.b_paractl").expect("bing result snippet selector")
});
static ANTI_BOT_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse(".anomaly-modal__modal").expect("anti-bot selector"));
static FORM_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("form").expect("form selector"));
static INPUT_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("input").expect("input selector"));
static SUBMIT_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("input[type='submit'], button[type='submit']").expect("submit selector")
});
static TITLE_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("title").expect("title selector"));
static BODY_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("body").expect("body selector"));
static META_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("meta").expect("meta selector"));
static JSON_LD_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("script[type='application/ld+json']").expect("json ld selector"));
static PREFERRED_CONTENT_SELECTORS: Lazy<Vec<Selector>> = Lazy::new(|| {
    [
        "main",
        "article",
        "[role='main']",
        "#main",
        ".main",
        ".content",
        ".article",
        ".post",
        ".post-content",
        ".entry-content",
        ".article-content",
        ".markdown-body",
        ".doc-content",
        ".docs-content",
        "section[itemprop='articleBody']",
        "body",
    ]
    .into_iter()
    .map(|raw| Selector::parse(raw).expect("content selector"))
    .collect()
});
static PARAGRAPHISH_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("p, li, pre, blockquote, h1, h2, h3, h4, h5, h6, tr")
        .expect("paragraphish selector")
});
static LINK_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("a").expect("link selector"));
static TABLE_CELL_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("th, td").expect("table cell selector"));

pub(crate) async fn search_with_fallback(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> Result<SearchOutcome, String> {
    let mut attempts = Vec::new();
    let mut html_search_succeeded = false;

    let (html_result, instant_result) = tokio::join!(
        timeout(
            Duration::from_secs(SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS),
            duckduckgo_html_search(client, query, limit),
        ),
        timeout(
            Duration::from_secs(SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS),
            duckduckgo_instant_answer_search(client, query, limit),
        ),
    );

    match html_result {
        Ok(Ok(hits)) if !hits.is_empty() => {
            return Ok(SearchOutcome {
                backend: NATIVE_SEARCH_BACKEND.to_string(),
                fallback_used: false,
                attempts,
                hits,
            });
        }
        Ok(Ok(_)) => {
            html_search_succeeded = true;
        }
        Ok(Err(err)) => attempts.push(ProviderAttempt {
            provider: "duckduckgo_html".to_string(),
            error: sanitize_provider_error(err),
        }),
        Err(_) => attempts.push(ProviderAttempt {
            provider: "duckduckgo_html".to_string(),
            error: "request timed out".to_string(),
        }),
    }

    match instant_result {
        Ok(Ok(hits)) if !hits.is_empty() => {
            return Ok(SearchOutcome {
                backend: NATIVE_SEARCH_BACKEND.to_string(),
                fallback_used: true,
                attempts,
                hits,
            });
        }
        Ok(Ok(_)) => {}
        Ok(Err(err)) => attempts.push(ProviderAttempt {
            provider: "duckduckgo_instant_answer".to_string(),
            error: sanitize_provider_error(err),
        }),
        Err(_) => attempts.push(ProviderAttempt {
            provider: "duckduckgo_instant_answer".to_string(),
            error: "request timed out".to_string(),
        }),
    }

    match timeout(
        Duration::from_secs(SEARCH_HTTP_STRATEGY_TIMEOUT_SECONDS),
        bing_html_search(client, query, limit),
    )
    .await
    {
        Ok(Ok(hits)) if !hits.is_empty() => {
            return Ok(SearchOutcome {
                backend: NATIVE_SEARCH_BACKEND.to_string(),
                fallback_used: true,
                attempts,
                hits,
            });
        }
        Ok(Ok(_)) => {}
        Ok(Err(err)) => attempts.push(ProviderAttempt {
            provider: "bing_html".to_string(),
            error: sanitize_provider_error(err),
        }),
        Err(_) => attempts.push(ProviderAttempt {
            provider: "bing_html".to_string(),
            error: "request timed out".to_string(),
        }),
    }

    if !html_search_succeeded {
        if let Some(options) = browser_options.filter(|_| limit > 0) {
            match timeout(
                Duration::from_secs(SEARCH_BROWSER_STRATEGY_TIMEOUT_SECONDS),
                duckduckgo_browser_search(query, limit, options),
            )
            .await
            {
                Ok(Ok(hits)) if !hits.is_empty() => {
                    return Ok(SearchOutcome {
                        backend: NATIVE_SEARCH_BACKEND.to_string(),
                        fallback_used: true,
                        attempts,
                        hits,
                    });
                }
                Ok(Ok(_)) => {}
                Ok(Err(err)) => attempts.push(ProviderAttempt {
                    provider: "duckduckgo_browser".to_string(),
                    error: sanitize_provider_error(err),
                }),
                Err(_) => attempts.push(ProviderAttempt {
                    provider: "duckduckgo_browser".to_string(),
                    error: "request timed out".to_string(),
                }),
            }
        }
    }

    if html_search_succeeded {
        return Ok(SearchOutcome {
            backend: NATIVE_SEARCH_BACKEND.to_string(),
            fallback_used: false,
            attempts,
            hits: Vec::new(),
        });
    }

    Err(format!(
        "web_search failed across internal strategies: {}",
        summarize_attempts(&attempts)
    ))
}

pub(crate) fn select_research_extract_urls(
    hits: &[SearchHit],
    desired: usize,
    max_extract_urls: usize,
) -> Vec<String> {
    let target = desired.min(max_extract_urls);
    if target == 0 {
        return Vec::new();
    }

    let candidates = hits
        .iter()
        .filter_map(|hit| build_research_url_candidate(hit.url.as_str()))
        .collect::<Vec<_>>();
    let mut selected = Vec::new();
    let mut seen_urls = HashSet::new();
    let mut seen_hosts = HashSet::new();

    for (require_article, reject_section, require_fresh_host) in [
        (true, false, true),
        (false, true, true),
        (false, false, true),
        (true, false, false),
        (false, true, false),
        (false, false, false),
    ] {
        for candidate in candidates.iter() {
            if selected.len() >= target {
                break;
            }
            if seen_urls.contains(candidate.url.as_str()) {
                continue;
            }
            if require_article && !candidate.article_like {
                continue;
            }
            if reject_section && candidate.section_like {
                continue;
            }
            if require_fresh_host
                && !candidate.host.is_empty()
                && seen_hosts.contains(candidate.host.as_str())
            {
                continue;
            }

            seen_urls.insert(candidate.url.clone());
            if !candidate.host.is_empty() {
                seen_hosts.insert(candidate.host.clone());
            }
            selected.push(candidate.url.clone());
        }
    }

    selected
}

fn build_research_url_candidate(raw_url: &str) -> Option<ResearchUrlCandidate> {
    let url = raw_url.trim();
    if url.is_empty() {
        return None;
    }

    let parsed = Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|value| value.host_str())
        .map(normalize_research_host)
        .unwrap_or_default();
    let segments = parsed
        .as_ref()
        .map(research_path_segments)
        .unwrap_or_default();
    let article_like = looks_like_article_path(&segments);
    let section_like = !article_like && looks_like_section_path(&segments);

    Some(ResearchUrlCandidate {
        url: url.to_string(),
        host,
        article_like,
        section_like,
    })
}

fn normalize_research_host(host: &str) -> String {
    host.strip_prefix("www.")
        .unwrap_or(host)
        .to_ascii_lowercase()
}

fn research_path_segments(url: &Url) -> Vec<String> {
    url.path_segments()
        .map(|segments| {
            segments
                .filter(|item| !item.is_empty())
                .map(|item| item.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn looks_like_article_path(segments: &[String]) -> bool {
    if segments.is_empty() {
        return false;
    }

    let slug_like_segments = segments
        .iter()
        .filter(|segment| looks_like_article_slug(segment.as_str()))
        .count();
    let total_path_chars = segments.iter().map(|segment| segment.len()).sum::<usize>();

    slug_like_segments > 0
        || has_date_like_segment(segments)
        || segments.len() >= 3
        || (segments.len() >= 2 && total_path_chars >= 40)
}

fn looks_like_article_slug(segment: &str) -> bool {
    let slug = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
    if slug.is_empty() {
        return false;
    }

    let alpha_chars = slug.chars().filter(|ch| ch.is_ascii_alphabetic()).count();
    let digit_chars = slug.chars().filter(|ch| ch.is_ascii_digit()).count();
    let has_separator = slug.contains('-') || slug.contains('_');

    (has_separator && slug.len() >= 16 && alpha_chars >= 8)
        || (slug.len() >= 28 && alpha_chars >= 12)
        || (digit_chars >= 6 && alpha_chars >= 6)
        || slug.ends_with("html")
}

fn has_date_like_segment(segments: &[String]) -> bool {
    let mut saw_year = false;

    for segment in segments {
        let value = segment.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        if value.len() == 4
            && value.chars().all(|ch| ch.is_ascii_digit())
            && (value.starts_with("19") || value.starts_with("20"))
        {
            saw_year = true;
            continue;
        }
        if saw_year && value.len() <= 2 && value.chars().all(|ch| ch.is_ascii_digit()) {
            return true;
        }
        if value.len() == 8 && value.chars().all(|ch| ch.is_ascii_digit()) {
            return true;
        }
    }

    false
}

fn looks_like_section_path(segments: &[String]) -> bool {
    if segments.is_empty() {
        return true;
    }

    if segments.len() == 1 {
        return true;
    }

    if segments.len() <= 4
        && segments
            .iter()
            .all(|segment| is_known_section_slug(segment.as_str()))
    {
        return true;
    }

    segments.len() <= 2
        && segments
            .last()
            .is_some_and(|segment| is_known_section_slug(segment.as_str()))
}

fn is_known_section_slug(segment: &str) -> bool {
    matches!(
        segment,
        "world"
            | "business"
            | "technology"
            | "tech"
            | "science"
            | "politics"
            | "markets"
            | "finance"
            | "news"
            | "latest"
            | "live"
            | "video"
            | "videos"
            | "opinion"
            | "sports"
            | "health"
            | "travel"
            | "lifestyle"
            | "culture"
            | "china"
            | "asia"
            | "europe"
            | "us"
            | "uk"
            | "global"
            | "topic"
            | "topics"
            | "tag"
            | "tags"
            | "category"
            | "categories"
            | "section"
            | "sections"
    )
}

pub(crate) async fn extract_with_fallback(
    client: &reqwest::Client,
    urls: &[String],
    max_extract_chars: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> Result<ExtractOutcome, String> {
    let pages = native_extract(client, urls, max_extract_chars, browser_options).await?;
    Ok(ExtractOutcome {
        backend: NATIVE_EXTRACT_BACKEND.to_string(),
        fallback_used: false,
        attempts: Vec::new(),
        pages,
    })
}

async fn duckduckgo_html_search(
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

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| format!("read response failed: {}", err))?;
        if !status.is_success() {
            return Err(format!(
                "status={} body={}",
                status,
                truncate_chars(&body, 800)
            ));
        }

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

fn parse_duckduckgo_html_results(body: &str, seen: &mut HashSet<String>) -> Vec<SearchHit> {
    let document = Html::parse_document(body);
    let mut hits = Vec::new();

    for block in document.select(&RESULT_BLOCK_SELECTOR) {
        let Some(link) = block.select(&RESULT_LINK_SELECTOR).next() else {
            continue;
        };

        let raw_url = link.value().attr("href").unwrap_or("").trim();
        let url = clean_search_result_url(raw_url, BING_SEARCH_ENDPOINT);
        if url.is_empty() || !seen.insert(url.clone()) {
            continue;
        }

        let title = normalize_whitespace(&link.text().collect::<Vec<_>>().join(" "));
        if title.is_empty() {
            continue;
        }

        let description = block
            .select(&RESULT_SNIPPET_SELECTOR)
            .next()
            .map(|item| normalize_whitespace(&item.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();

        hits.push(SearchHit {
            url,
            title,
            description,
        });
    }

    hits
}

async fn bing_html_search(
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

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("read response failed: {}", err))?;
    if !status.is_success() {
        return Err(format!(
            "status={} body={}",
            status,
            truncate_chars(&body, 800)
        ));
    }

    Ok(parse_bing_html_results(&body, limit))
}

fn parse_bing_html_results(body: &str, limit: usize) -> Vec<SearchHit> {
    let document = Html::parse_document(body);
    let mut hits = Vec::new();
    let mut seen = HashSet::new();

    for block in document.select(&BING_RESULT_BLOCK_SELECTOR) {
        let Some(link) = block.select(&BING_RESULT_LINK_SELECTOR).next() else {
            continue;
        };

        let raw_url = link
            .value()
            .attr("href")
            .or_else(|| link.value().attr("data-href"))
            .unwrap_or("")
            .trim();
        let url = clean_duckduckgo_result_url(raw_url);
        if url.is_empty() || !seen.insert(url.clone()) {
            continue;
        }

        let title = normalize_whitespace(&link.text().collect::<Vec<_>>().join(" "));
        if title.is_empty() {
            continue;
        }

        let description = block
            .select(&BING_RESULT_SNIPPET_SELECTOR)
            .next()
            .map(|item| normalize_whitespace(&item.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();

        hits.push(SearchHit {
            url,
            title,
            description,
        });
        if hits.len() >= limit {
            break;
        }
    }

    hits
}

fn extract_duckduckgo_next_page_form(body: &str) -> Option<Vec<(String, String)>> {
    let document = Html::parse_document(body);

    for form in document.select(&FORM_SELECTOR) {
        let has_next_button = form.select(&SUBMIT_SELECTOR).any(|button| {
            let value = button.value().attr("value").unwrap_or("");
            if value.eq_ignore_ascii_case("Next") {
                return true;
            }
            normalize_whitespace(&button.text().collect::<Vec<_>>().join(" "))
                .eq_ignore_ascii_case("Next")
        });

        if !has_next_button {
            continue;
        }

        let params = form
            .select(&INPUT_SELECTOR)
            .filter_map(|input| {
                let name = input.value().attr("name")?.trim();
                if name.is_empty() {
                    return None;
                }
                let value = input.value().attr("value").unwrap_or("").trim();
                Some((name.to_string(), value.to_string()))
            })
            .collect::<Vec<_>>();
        if !params.is_empty() {
            return Some(params);
        }
    }

    None
}

fn looks_like_duckduckgo_antibot(body: &str) -> bool {
    let document = Html::parse_document(body);
    if document.select(&ANTI_BOT_SELECTOR).next().is_some() {
        return true;
    }

    let normalized = body.to_ascii_lowercase();
    normalized.contains("anomaly-modal")
        || normalized.contains("automated requests")
        || normalized.contains("duckduckgo detected unusual traffic")
}

fn clean_search_result_url(raw: &str, base_url: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let parsed =
        Url::parse(trimmed).or_else(|_| Url::parse(base_url).and_then(|base| base.join(trimmed)));
    let Ok(parsed) = parsed else {
        return trimmed.to_string();
    };

    if !matches!(parsed.scheme(), "http" | "https") {
        return String::new();
    }

    if let Some(target) = parsed.query_pairs().find_map(|(key, value)| {
        if key == "uddg" {
            Some(value.into_owned())
        } else {
            None
        }
    }) {
        return target;
    }

    parsed.to_string()
}

fn clean_duckduckgo_result_url(raw: &str) -> String {
    clean_search_result_url(raw, "https://duckduckgo.com")
}

async fn duckduckgo_instant_answer_search(
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

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("read response failed: {}", err))?;
    if !status.is_success() {
        return Err(format!(
            "status={} body={}",
            status,
            truncate_chars(&text, 800)
        ));
    }

    let value = serde_json::from_str::<Value>(&text).map_err(|err| {
        format!(
            "invalid JSON response: {} body={}",
            err,
            truncate_chars(&text, 800)
        )
    })?;

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let abstract_url = value
        .get("AbstractURL")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    if !abstract_url.is_empty() {
        let title = value
            .get("Heading")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let description = value
            .get("AbstractText")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        seen.insert(abstract_url.to_string());
        out.push(SearchHit {
            url: abstract_url.to_string(),
            title,
            description,
        });
    }

    if let Some(related) = value.get("RelatedTopics").and_then(|item| item.as_array()) {
        for item in related {
            append_duckduckgo_hit(item, &mut out, &mut seen, limit);
            if out.len() >= limit {
                break;
            }
        }
    }

    if out.len() > limit {
        out.truncate(limit);
    }
    Ok(out)
}

async fn duckduckgo_browser_search(
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

    let session = new_browser_session();
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
    let open_result = run_browser_command(
        &options.workspace_dir,
        &session,
        "open",
        vec![search_url],
        open_timeout,
    )
    .await?;
    if !is_browser_command_success(&open_result) {
        return Err(browser_command_error(
            &open_result,
            "browser search page failed to open",
        ));
    }

    sleep(Duration::from_millis(BROWSER_SEARCH_SETTLE_DELAY_MS)).await;

    let eval_result = run_browser_command(
        &options.workspace_dir,
        &session,
        "eval",
        vec![browser_search_eval_expression(limit)],
        command_timeout,
    )
    .await?;
    let _ = run_browser_command(&options.workspace_dir, &session, "close", Vec::new(), 10).await;

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

    if parsed
        .get("anti_bot")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Err("browser search hit an anti-bot or challenge page".to_string());
    }

    Ok(parsed
        .get("hits")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(parse_browser_search_hit)
                .take(limit)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

fn parse_browser_search_hit(value: &Value) -> Option<SearchHit> {
    let url = value
        .get("url")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(clean_duckduckgo_result_url)?;
    let title = value
        .get("title")
        .and_then(|item| item.as_str())
        .map(normalize_whitespace)
        .filter(|item| !item.is_empty())?;
    let description = value
        .get("description")
        .and_then(|item| item.as_str())
        .map(normalize_whitespace)
        .unwrap_or_default();

    Some(SearchHit {
        url,
        title,
        description,
    })
}

fn browser_search_eval_expression(limit: usize) -> String {
    format!(
        r##"JSON.stringify((() => {{
  const clean = (text) => (text || "").replace(/\u00a0/g, " ").replace(/\s+/g, " ").trim();
  const decodeHref = (raw) => {{
    const href = clean(raw);
    if (!href) return "";
    try {{
      const url = new URL(href, window.location.href);
      const uddg = url.searchParams.get("uddg");
      return uddg ? decodeURIComponent(uddg) : url.toString();
    }} catch (_err) {{
      return href;
    }}
  }};
  const antiBotText = clean(document.body?.innerText || "").toLowerCase();
  const antiBot = antiBotText.includes("automated requests")
    || antiBotText.includes("unusual traffic")
    || antiBotText.includes("anomaly-modal");
  const blocks = Array.from(document.querySelectorAll(".result, .result.web-result"));
  const hits = [];
  const seen = new Set();
  for (const block of blocks) {{
    const link = block.querySelector("a.result__a, a.result-link, h2 a, .links_main a");
    if (!link) continue;
    const url = decodeHref(link.getAttribute("href") || link.href || "");
    const title = clean(link.textContent || link.innerText || "");
    if (!url || !title || seen.has(url)) continue;
    seen.add(url);
    const description = clean(
      block.querySelector(".result__snippet, .result-snippet, .snippet, .result__extras__body")?.textContent
      || ""
    );
    hits.push({{ url, title, description }});
    if (hits.length >= {limit}) break;
  }}
  return {{ anti_bot: antiBot, hits }};
}})())"##,
        limit = limit.max(1)
    )
}

fn append_duckduckgo_hit(
    value: &Value,
    out: &mut Vec<SearchHit>,
    seen: &mut HashSet<String>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }

    if let Some(children) = value.get("Topics").and_then(|item| item.as_array()) {
        for child in children {
            append_duckduckgo_hit(child, out, seen, limit);
            if out.len() >= limit {
                break;
            }
        }
        return;
    }

    let url = value
        .get("FirstURL")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    if url.is_empty() {
        return;
    }

    if !seen.insert(url.to_string()) {
        return;
    }

    let description = value
        .get("Text")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let title = guess_title(description.as_str(), url);
    out.push(SearchHit {
        url: url.to_string(),
        title,
        description,
    });
}

async fn native_extract(
    client: &reqwest::Client,
    urls: &[String],
    max_extract_chars: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> Result<Vec<ExtractedPage>, String> {
    let mut pages = Vec::new();
    let browser_render_available =
        browser_options.is_some_and(|_| browser_backend_available().is_ok());

    for source_url in urls {
        let response = match client
            .get(source_url)
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,text/plain;q=0.8,application/json;q=0.8,application/pdf;q=0.7,*/*;q=0.5",
            )
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                pages.push(error_page(
                    source_url.to_string(),
                    source_url.to_string(),
                    format!("request failed: {}", err),
                ));
                continue;
            }
        };

        let status = response.status();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());

        if !status.is_success() {
            pages.push(error_page(
                source_url.to_string(),
                final_url,
                format!("status={}", status),
            ));
            continue;
        }

        let body = match response.bytes().await {
            Ok(body) => body,
            Err(err) => {
                pages.push(error_page(
                    source_url.to_string(),
                    final_url,
                    format!("read response failed: {}", err),
                ));
                continue;
            }
        };

        let kind = detect_response_content_kind(content_type.as_deref(), source_url, &body);
        let page = match kind {
            ResponseContentKind::Html => {
                let static_page = extract_html_page(&final_url, &body, max_extract_chars);
                if browser_render_available
                    && should_try_browser_render(&String::from_utf8_lossy(&body), &static_page)
                {
                    match browser_options {
                        Some(options) => {
                            match browser_render_extract(
                                final_url.as_str(),
                                max_extract_chars,
                                options,
                            )
                            .await
                            {
                                Ok(Some(rendered)) => maybe_upgrade_html_page(
                                    static_page,
                                    rendered,
                                    max_extract_chars,
                                ),
                                Ok(None) | Err(_) => static_page,
                            }
                        }
                        None => static_page,
                    }
                } else {
                    static_page
                }
            }
            ResponseContentKind::Json => extract_json_page(&final_url, &body, max_extract_chars),
            ResponseContentKind::Text => extract_text_page(&final_url, &body, max_extract_chars),
            ResponseContentKind::Pdf => extract_pdf_page(&final_url, &body, max_extract_chars),
            ResponseContentKind::Unsupported(kind) => error_page(
                source_url.to_string(),
                final_url,
                format!("unsupported content type: {}", kind),
            ),
        };
        pages.push(page);
    }

    Ok(pages)
}

async fn browser_render_extract(
    url: &str,
    max_extract_chars: usize,
    options: &BrowserRenderOptions,
) -> Result<Option<BrowserRenderedPage>, String> {
    let session = new_browser_session();
    let open_result = run_browser_command(
        &options.workspace_dir,
        &session,
        "open",
        vec![url.to_string()],
        options
            .command_timeout_seconds
            .max(BROWSER_RENDER_OPEN_TIMEOUT_SECONDS),
    )
    .await?;
    if !is_browser_command_success(&open_result) {
        return Ok(None);
    }

    let snapshot_result = run_browser_command(
        &options.workspace_dir,
        &session,
        "snapshot",
        vec!["-c".to_string()],
        options.command_timeout_seconds,
    )
    .await?;
    let snapshot = snapshot_result
        .get("data")
        .and_then(|value| value.get("snapshot"))
        .and_then(|value| value.as_str())
        .map(|value| truncate_chars(value, max_extract_chars))
        .unwrap_or_default();

    let eval_result = run_browser_command(
        &options.workspace_dir,
        &session,
        "eval",
        vec![browser_render_eval_expression()],
        options.command_timeout_seconds,
    )
    .await?;
    if !is_browser_command_success(&eval_result) {
        return Ok(None);
    }

    let parsed = eval_result
        .get("data")
        .and_then(|value| value.get("result"))
        .cloned()
        .map(parse_browser_eval_result)
        .unwrap_or(Value::Null);
    let content = parsed
        .get("content")
        .and_then(|value| value.as_str())
        .map(normalize_multiline_text)
        .unwrap_or_default();
    let body_text = parsed
        .get("body_text")
        .and_then(|value| value.as_str())
        .map(normalize_multiline_text)
        .unwrap_or_default();
    let merged_content = merge_content_candidates(
        first_non_empty_owned(&[content.as_str(), body_text.as_str()]),
        parsed
            .get("meta_description")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        "",
        &[snapshot.clone()],
    );

    if merged_content.trim().is_empty() && snapshot.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(BrowserRenderedPage {
        url: parsed
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or(url)
            .to_string(),
        title: parsed
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        content: merged_content,
        meta_description: parsed
            .get("meta_description")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        snapshot,
    }))
}

fn maybe_upgrade_html_page(
    static_page: ExtractedPage,
    rendered: BrowserRenderedPage,
    max_extract_chars: usize,
) -> ExtractedPage {
    let BrowserRenderedPage {
        url,
        title,
        content,
        meta_description,
        snapshot,
    } = rendered;
    let browser_content = merge_content_candidates(
        content,
        meta_description.as_str(),
        "",
        &[static_page.content.clone(), snapshot],
    );
    if browser_content.trim().is_empty() {
        return static_page;
    }

    let static_score = content_quality_score(static_page.content.as_str());
    let browser_score = content_quality_score(browser_content.as_str());
    let static_chars = static_page.original_content_chars;
    let browser_chars = browser_content.chars().count();
    let should_replace = static_chars < MIN_BROWSER_RENDER_TRIGGER_CHARS
        || browser_score > static_score.saturating_add(180)
        || (browser_chars >= static_chars.saturating_add(220)
            && browser_score >= static_score.saturating_add(80));

    if !should_replace {
        return static_page;
    }

    let title = first_non_empty_owned(&[title.as_str(), static_page.title.as_str(), url.as_str()]);
    finalize_page(url, title, browser_content, max_extract_chars)
}

fn should_try_browser_render(raw_html: &str, page: &ExtractedPage) -> bool {
    let content = page.content.trim();
    if content.is_empty() {
        return true;
    }

    let normalized = normalize_whitespace(content).to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }

    let weak_markers = [
        "enable javascript",
        "javascript is required",
        "requires javascript",
        "please turn on javascript",
        "checking your browser",
        "just a moment",
        "loading",
        "please wait",
    ];
    if weak_markers
        .iter()
        .any(|marker| normalized.contains(marker))
    {
        return true;
    }

    let chars = content.chars().count();
    if chars < MIN_BROWSER_RENDER_TRIGGER_CHARS {
        return true;
    }

    let non_empty_lines = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let sentenceish = normalized.matches('.').count()
        + normalized.matches('!').count()
        + normalized.matches('?').count();
    let navigation_hits = [
        "home",
        "docs",
        "pricing",
        "blog",
        "contact",
        "menu",
        "navigation",
        "sign in",
        "log in",
        "get started",
    ]
    .iter()
    .filter(|needle| normalized.contains(**needle))
    .count();
    let raw_lower = raw_html.to_ascii_lowercase();
    let spa_shell_markers = [
        "id=\"__next\"",
        "id=\"app\"",
        "id=\"root\"",
        "data-reactroot",
        "ng-version",
        "webpack",
        "vite",
        "__nuxt",
    ];
    let has_spa_shell = spa_shell_markers
        .iter()
        .any(|marker| raw_lower.contains(marker));
    let script_count = raw_lower.matches("<script").count();
    let looks_sparse = non_empty_lines <= 4 && sentenceish <= 2;
    let looks_navigation_heavy = chars < 700 && navigation_hits >= 4 && sentenceish <= 2;

    (has_spa_shell || script_count >= 6) && (looks_sparse || looks_navigation_heavy)
}

fn content_quality_score(content: &str) -> usize {
    let normalized = normalize_multiline_text(content);
    if normalized.is_empty() {
        return 0;
    }

    let chars = normalized.chars().count().min(5_000);
    let paragraphs = normalized
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let sentenceish = normalized.matches('.').count()
        + normalized.matches('!').count()
        + normalized.matches('?').count();
    let weak_hits = [
        "enable javascript",
        "javascript is required",
        "please wait",
        "loading",
        "sign in",
        "log in",
        "menu",
        "navigation",
    ]
    .iter()
    .filter(|needle| normalized.to_ascii_lowercase().contains(**needle))
    .count();

    chars
        .saturating_add(paragraphs.saturating_mul(50))
        .saturating_add(sentenceish.saturating_mul(35))
        .saturating_sub(weak_hits.saturating_mul(90))
}

fn browser_render_eval_expression() -> String {
    r##"JSON.stringify((() => {
  const selectorList = [
    "main",
    "article",
    "[role='main']",
    "#main",
    ".main",
    ".content",
    ".article",
    ".post",
    ".post-content",
    ".entry-content",
    ".article-content",
    ".markdown-body",
    ".doc-content",
    ".docs-content",
    "section[itemprop='articleBody']"
  ];
  const blockSelector = "p, li, pre, blockquote, h1, h2, h3, h4, h5, h6, tr";
  const noisePattern = /(header|footer|nav|sidebar|cookie|modal|popup|social|share|breadcrumb|advert|banner|menu|toolbar)/i;
  const clean = (text) => (text || "").replace(/\u00a0/g, " ").replace(/\s+/g, " ").trim();
  const isNoise = (element) => {
    if (!element) return false;
    const tag = (element.tagName || "").toLowerCase();
    if (["header", "footer", "nav", "aside", "form", "dialog"].includes(tag)) {
      return true;
    }
    const attrs = [
      element.id || "",
      element.className || "",
      element.getAttribute("role") || "",
      element.getAttribute("aria-label") || "",
      element.getAttribute("data-testid") || ""
    ].join(" ");
    if (/(article|content|markdown|docs|main|post)/i.test(attrs)) {
      return false;
    }
    return noisePattern.test(attrs);
  };
  const formatBlock = (element) => {
    const tag = (element.tagName || "").toLowerCase();
    if (tag === "tr") {
      const cells = Array.from(element.querySelectorAll("th,td"))
        .map((cell) => clean(cell.innerText))
        .filter(Boolean);
      if (cells.length > 0) {
        return cells.join(" | ");
      }
    }
    const text = clean(element.innerText);
    if (!text) return "";
    if (tag === "li") return `- ${text}`;
    if (tag === "blockquote") return `> ${text}`;
    return text;
  };
  const collectBlocks = (root) => {
    if (!root) return "";
    const blocks = [];
    for (const element of root.querySelectorAll(blockSelector)) {
      if (isNoise(element)) continue;
      const text = formatBlock(element);
      if (!text) continue;
      if (blocks[blocks.length - 1] === text) continue;
      blocks.push(text);
    }
    if (blocks.length === 0) {
      return clean(root.innerText);
    }
    return blocks.join("\n\n").trim();
  };
  const score = (root, text) => {
    if (!text) return 0;
    const tag = (root.tagName || "").toLowerCase();
    const paragraphCount = root.querySelectorAll(blockSelector).length;
    const linkChars = Array.from(root.querySelectorAll("a"))
      .map((link) => clean(link.innerText).length)
      .reduce((sum, value) => sum + value, 0);
    const tagBonus = tag === "main" ? 250 : tag === "article" ? 220 : tag === "section" ? 120 : tag === "body" ? 0 : 80;
    const attrs = [
      root.id || "",
      root.className || "",
      root.getAttribute("role") || "",
      root.getAttribute("itemprop") || ""
    ].join(" ");
    const attrBonus = /(content|article|post|markdown|docs|main|body)/i.test(attrs) ? 180 : 0;
    return text.length + (paragraphCount * 60) + tagBonus + attrBonus - Math.floor(linkChars / 3);
  };
  const candidates = [];
  for (const selector of selectorList) {
    for (const element of document.querySelectorAll(selector)) {
      if (isNoise(element)) continue;
      const text = collectBlocks(element);
      if (text.length < 120) continue;
      candidates.push({ text, score: score(element, text) });
    }
  }
  candidates.sort((left, right) => right.score - left.score);
  const best = candidates[0]?.text || "";
  return {
    url: window.location.href,
    title: document.title || "",
    meta_description:
      document.querySelector("meta[name='description']")?.getAttribute("content") ||
      document.querySelector("meta[property='og:description']")?.getAttribute("content") ||
      "",
    content: best,
    body_text: clean(document.body?.innerText || "")
  };
})())"##.to_string()
}

fn parse_browser_eval_result(raw: Value) -> Value {
    if let Some(text) = raw.as_str() {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
    } else {
        raw
    }
}

fn is_browser_command_success(value: &Value) -> bool {
    value
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn detect_response_content_kind(
    content_type: Option<&str>,
    source_url: &str,
    body: &[u8],
) -> ResponseContentKind {
    let mime = content_type
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    if mime == "application/pdf"
        || source_url.to_ascii_lowercase().ends_with(".pdf")
        || body.starts_with(b"%PDF-")
    {
        return ResponseContentKind::Pdf;
    }

    if mime.contains("html") || mime.contains("xhtml") {
        return ResponseContentKind::Html;
    }
    if mime.contains("json") {
        return ResponseContentKind::Json;
    }
    if mime.starts_with("text/")
        || mime.contains("xml")
        || mime.contains("javascript")
        || mime.contains("markdown")
        || mime.contains("csv")
    {
        if bytes_look_like_html(body) {
            return ResponseContentKind::Html;
        }
        return ResponseContentKind::Text;
    }

    if mime.is_empty() {
        if bytes_look_like_html(body) {
            return ResponseContentKind::Html;
        }
        if bytes_look_like_json(body) {
            return ResponseContentKind::Json;
        }
        if std::str::from_utf8(body).is_ok() {
            return ResponseContentKind::Text;
        }
        return ResponseContentKind::Unsupported("binary".to_string());
    }

    ResponseContentKind::Unsupported(mime)
}

fn extract_html_page(final_url: &str, body: &[u8], max_extract_chars: usize) -> ExtractedPage {
    let html = String::from_utf8_lossy(body).to_string();
    let raw_document = Html::parse_document(&html);
    let metadata = extract_html_metadata(&raw_document);
    let structured = extract_structured_data_content(&raw_document);
    let cleaned_html = clean_html_for_extraction(&html);
    let document = Html::parse_document(&cleaned_html);
    let mut content = extract_main_content_text(&document);
    if content.is_empty() {
        content = html_to_text(&cleaned_html);
    }
    content = merge_content_candidates(
        content,
        metadata.description.as_str(),
        structured.description.as_str(),
        structured.text_blocks.as_slice(),
    );

    finalize_page(
        final_url.to_string(),
        choose_page_title(final_url, &metadata, &structured),
        content,
        max_extract_chars,
    )
}

fn extract_json_page(final_url: &str, body: &[u8], max_extract_chars: usize) -> ExtractedPage {
    let content = match serde_json::from_slice::<Value>(body) {
        Ok(value) => serde_json::to_string_pretty(&value)
            .unwrap_or_else(|_| String::from_utf8_lossy(body).to_string()),
        Err(_) => String::from_utf8_lossy(body).to_string(),
    };
    finalize_page(
        final_url.to_string(),
        guess_title_from_url(final_url),
        content,
        max_extract_chars,
    )
}

fn extract_text_page(final_url: &str, body: &[u8], max_extract_chars: usize) -> ExtractedPage {
    let raw = String::from_utf8_lossy(body).to_string();
    let content = if raw.contains('<') && raw.contains('>') {
        html_to_text(&raw)
    } else {
        normalize_multiline_text(&raw)
    };
    finalize_page(
        final_url.to_string(),
        guess_title_from_url(final_url),
        content,
        max_extract_chars,
    )
}

fn extract_pdf_page(final_url: &str, body: &[u8], max_extract_chars: usize) -> ExtractedPage {
    match extract_pdf_text(body) {
        Some(content) => finalize_page(
            final_url.to_string(),
            guess_title_from_url(final_url),
            content,
            max_extract_chars,
        ),
        None => error_page(
            final_url.to_string(),
            final_url.to_string(),
            "unable to extract text from PDF".to_string(),
        ),
    }
}

fn finalize_page(
    url: String,
    title: String,
    mut content: String,
    max_extract_chars: usize,
) -> ExtractedPage {
    content = normalize_multiline_text(&content);
    let original_content_chars = content.chars().count();
    let truncated = original_content_chars > max_extract_chars;
    let content_summary = if truncated {
        Some(build_extract_summary(content.as_str(), max_extract_chars))
    } else {
        None
    };
    if truncated {
        content = truncate_chars(content.as_str(), max_extract_chars);
    }
    let content_chars = content.chars().count();

    ExtractedPage {
        url,
        title,
        content,
        content_chars,
        original_content_chars,
        truncated,
        content_summary,
        error: None,
    }
}

fn error_page(requested_url: String, final_url: String, error: String) -> ExtractedPage {
    let url = if final_url.trim().is_empty() {
        requested_url
    } else {
        final_url
    };
    ExtractedPage {
        url,
        title: String::new(),
        content: String::new(),
        content_chars: 0,
        original_content_chars: 0,
        truncated: false,
        content_summary: None,
        error: Some(error),
    }
}

fn clean_html_for_extraction(html: &str) -> String {
    let mut cleaned = html.to_string();
    for pattern in [
        &*RE_SCRIPT_TAG,
        &*RE_STYLE_TAG,
        &*RE_NOSCRIPT_TAG,
        &*RE_TEMPLATE_TAG,
        &*RE_COMMENT_TAG,
    ] {
        cleaned = pattern.replace_all(&cleaned, " ").to_string();
    }

    for _ in 0..2 {
        cleaned = RE_LAYOUT_TAG.replace_all(&cleaned, " ").to_string();
        cleaned = RE_NOISE_BLOCK.replace_all(&cleaned, " ").to_string();
    }

    cleaned
}

fn extract_html_metadata(document: &Html) -> HtmlMetadata {
    let title = document
        .select(&TITLE_SELECTOR)
        .next()
        .map(|item| normalize_whitespace(&item.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_default();

    let mut description = String::new();
    let mut og_title = String::new();

    for meta in document.select(&META_SELECTOR) {
        let content = meta.value().attr("content").unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }

        let name = meta
            .value()
            .attr("name")
            .or_else(|| meta.value().attr("property"))
            .or_else(|| meta.value().attr("itemprop"))
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        match name.as_str() {
            "description" | "og:description" if description.is_empty() => {
                description = normalize_whitespace(content);
            }
            "og:title" if og_title.is_empty() => {
                og_title = normalize_whitespace(content);
            }
            _ => {}
        }
    }

    HtmlMetadata {
        title: first_non_empty_owned(&[og_title.as_str(), title.as_str()]),
        description,
    }
}

fn extract_structured_data_content(document: &Html) -> StructuredDataContent {
    let mut out = StructuredDataContent::default();
    let mut seen = HashSet::new();

    for script in document.select(&JSON_LD_SELECTOR) {
        let raw = script.text().collect::<Vec<_>>().join("");
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        collect_structured_data_value(&value, &mut out, &mut seen, false);
    }

    out
}

fn collect_structured_data_value(
    value: &Value,
    out: &mut StructuredDataContent,
    seen: &mut HashSet<String>,
    in_contentish_node: bool,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_structured_data_value(item, out, seen, in_contentish_node);
            }
        }
        Value::Object(map) => {
            let type_hint = map
                .get("@type")
                .map(extract_json_string_values)
                .unwrap_or_default()
                .join(" ")
                .to_ascii_lowercase();
            let contentish = in_contentish_node || looks_like_content_type(&type_hint);

            if out.title.is_empty() {
                for key in ["headline", "name", "title"] {
                    if let Some(text) = map.get(key).and_then(normalized_json_string) {
                        out.title = text;
                        break;
                    }
                }
            }

            if out.description.is_empty() {
                for key in ["description", "abstract"] {
                    if let Some(text) = map.get(key).and_then(normalized_json_string) {
                        out.description = text;
                        break;
                    }
                }
            }

            for key in ["articleBody", "text", "description"] {
                if let Some(candidate) = map.get(key).and_then(normalized_json_string) {
                    let long_enough = candidate.chars().count() >= 80;
                    let should_keep = key == "articleBody" || (contentish && long_enough);
                    if should_keep {
                        push_unique_text_block(&mut out.text_blocks, seen, candidate);
                    }
                }
            }

            for child in map.values() {
                collect_structured_data_value(child, out, seen, contentish);
            }
        }
        _ => {}
    }
}

fn looks_like_content_type(type_hint: &str) -> bool {
    [
        "article",
        "newsarticle",
        "blogposting",
        "webpage",
        "techarticle",
        "report",
        "documentation",
        "faqpage",
        "howto",
        "product",
    ]
    .iter()
    .any(|needle| type_hint.contains(needle))
}

fn extract_json_string_values(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => vec![text.to_string()],
        Value::Array(items) => items.iter().flat_map(extract_json_string_values).collect(),
        _ => Vec::new(),
    }
}

fn normalized_json_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let normalized = normalize_multiline_text(&decode_basic_html_entities(text));
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        }
        _ => None,
    }
}

fn merge_content_candidates(
    primary_content: String,
    metadata_description: &str,
    structured_description: &str,
    structured_blocks: &[String],
) -> String {
    let mut sections = Vec::new();

    if !primary_content.trim().is_empty() {
        sections.push(primary_content.trim().to_string());
    }

    let current_chars = sections
        .iter()
        .map(|item| item.chars().count())
        .sum::<usize>();

    if current_chars < 400 {
        for block in structured_blocks {
            let trimmed = block.trim();
            if trimmed.is_empty() {
                continue;
            }
            if sections
                .iter()
                .any(|existing| text_contains(existing, trimmed))
            {
                continue;
            }
            sections.push(trimmed.to_string());
        }
    }

    if sections
        .iter()
        .map(|item| item.chars().count())
        .sum::<usize>()
        < 220
    {
        for description in [metadata_description, structured_description] {
            let trimmed = description.trim();
            if trimmed.is_empty() {
                continue;
            }
            if sections
                .iter()
                .any(|existing| text_contains(existing, trimmed))
            {
                continue;
            }
            sections.insert(0, trimmed.to_string());
        }
    }

    normalize_multiline_text(&sections.join("\n\n"))
}

fn push_unique_text_block(out: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    let normalized_key = normalize_whitespace(&value).to_ascii_lowercase();
    if normalized_key.is_empty() || !seen.insert(normalized_key) {
        return;
    }
    out.push(value);
}

fn text_contains(haystack: &str, needle: &str) -> bool {
    let normalized_haystack = normalize_whitespace(haystack).to_ascii_lowercase();
    let normalized_needle = normalize_whitespace(needle).to_ascii_lowercase();
    normalized_haystack.contains(normalized_needle.as_str())
}

fn choose_page_title(
    final_url: &str,
    metadata: &HtmlMetadata,
    structured: &StructuredDataContent,
) -> String {
    let guessed = guess_title_from_url(final_url);
    first_non_empty_owned(&[
        metadata.title.as_str(),
        structured.title.as_str(),
        guessed.as_str(),
    ])
}

fn extract_main_content_text(document: &Html) -> String {
    let body_text = document
        .select(&BODY_SELECTOR)
        .next()
        .map(|element| collect_candidate_text(&element))
        .unwrap_or_default();

    let mut best_text = String::new();
    let mut best_score = 0usize;

    for selector in PREFERRED_CONTENT_SELECTORS.iter() {
        for element in document.select(selector) {
            if is_probably_noise_element(&element) {
                continue;
            }

            let text = collect_candidate_text(&element);
            let text_chars = text.chars().count();
            if text_chars < 120 {
                continue;
            }

            let score = score_content_candidate(&element, text_chars);
            if score > best_score {
                best_score = score;
                best_text = text;
            }
        }
    }

    if best_text.is_empty() {
        return body_text;
    }

    let body_len = body_text.chars().count();
    let best_len = best_text.chars().count();
    if body_len > 0 && best_len * 100 / body_len < 20 {
        body_text
    } else {
        best_text
    }
}

fn flatten_element_text(element: &ElementRef<'_>) -> String {
    let text = element.text().collect::<Vec<_>>().join(" ");
    normalize_multiline_text(&decode_basic_html_entities(&text))
}

fn collect_candidate_text(element: &ElementRef<'_>) -> String {
    let mut blocks = Vec::new();

    for block in element.select(&PARAGRAPHISH_SELECTOR) {
        if is_probably_noise_element(&block) {
            continue;
        }

        let formatted = format_block_text(&block);
        if formatted.is_empty() {
            continue;
        }
        if blocks
            .last()
            .is_some_and(|last: &String| last == &formatted)
        {
            continue;
        }
        blocks.push(formatted);
    }

    if blocks.is_empty() {
        return flatten_element_text(element);
    }

    normalize_multiline_text(&blocks.join("\n\n"))
}

fn format_block_text(element: &ElementRef<'_>) -> String {
    let name = element.value().name();
    let text = if name == "tr" {
        let cells = element
            .select(&TABLE_CELL_SELECTOR)
            .map(|cell| flatten_element_text(&cell))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        if cells.is_empty() {
            flatten_element_text(element)
        } else {
            cells.join(" | ")
        }
    } else {
        flatten_element_text(element)
    };

    if text.is_empty() {
        return String::new();
    }

    match name {
        "li" => format!("- {}", text),
        "blockquote" => format!("> {}", text),
        _ => text,
    }
}

fn score_content_candidate(element: &ElementRef<'_>, text_chars: usize) -> usize {
    let paragraph_count = element.select(&PARAGRAPHISH_SELECTOR).count();
    let link_chars = element
        .select(&LINK_SELECTOR)
        .map(|item| flatten_element_text(&item))
        .map(|text| text.chars().count())
        .sum::<usize>();
    let tag_bonus = match element.value().name() {
        "main" => 250,
        "article" => 220,
        "section" => 120,
        "body" => 0,
        _ => 80,
    };
    let attr_bonus = if looks_like_main_content_attr(element) {
        180
    } else {
        0
    };

    text_chars
        .saturating_add(paragraph_count.saturating_mul(60))
        .saturating_add(tag_bonus)
        .saturating_add(attr_bonus)
        .saturating_sub(link_chars / 3)
}

fn looks_like_main_content_attr(element: &ElementRef<'_>) -> bool {
    let attrs = [
        element.value().attr("id"),
        element.value().attr("class"),
        element.value().attr("role"),
        element.value().attr("itemprop"),
    ];
    attrs.into_iter().flatten().any(|raw| {
        let value = raw.to_ascii_lowercase();
        [
            "content", "article", "post", "markdown", "docs", "main", "body",
        ]
        .iter()
        .any(|needle| value.contains(needle))
    })
}

fn is_probably_noise_element(element: &ElementRef<'_>) -> bool {
    let tag = element.value().name();
    if matches!(
        tag,
        "header" | "footer" | "nav" | "aside" | "form" | "dialog"
    ) {
        return true;
    }

    let attrs = [
        element.value().attr("id"),
        element.value().attr("class"),
        element.value().attr("role"),
        element.value().attr("aria-label"),
        element.value().attr("data-testid"),
    ];
    attrs.into_iter().flatten().any(|raw| {
        let value = raw.to_ascii_lowercase();
        let positive = ["article", "content", "markdown", "docs", "main", "post"]
            .iter()
            .any(|needle| value.contains(needle));
        if positive {
            return false;
        }

        [
            "header",
            "footer",
            "nav",
            "sidebar",
            "cookie",
            "modal",
            "popup",
            "social",
            "share",
            "breadcrumb",
            "advert",
            "banner",
            "menu",
            "toolbar",
        ]
        .iter()
        .any(|needle| value.contains(needle))
    })
}

fn bytes_look_like_html(body: &[u8]) -> bool {
    let preview = String::from_utf8_lossy(&body[..body.len().min(1024)]).to_ascii_lowercase();
    preview.contains("<!doctype html")
        || preview.contains("<html")
        || preview.contains("<body")
        || preview.contains("<main")
        || preview.contains("<article")
}

fn bytes_look_like_json(body: &[u8]) -> bool {
    let preview = String::from_utf8_lossy(&body[..body.len().min(256)]);
    let trimmed = preview.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn extract_pdf_text(data: &[u8]) -> Option<String> {
    let mut text = None;

    #[cfg(feature = "pdf")]
    {}

    if text.is_none() {
        if let Ok(doc) = lopdf::Document::load_mem(data) {
            let pages: Vec<u32> = doc.get_pages().keys().cloned().collect();
            if let Ok(value) = doc.extract_text(&pages) {
                let normalized = normalize_multiline_text(&value);
                if !normalized.is_empty() {
                    text = Some(normalized);
                }
            }
        }
    }

    text
}

fn summarize_attempts(attempts: &[ProviderAttempt]) -> String {
    if attempts.is_empty() {
        return "no strategy attempted".to_string();
    }
    attempts
        .iter()
        .map(|item| format!("{}: {}", item.provider, item.error))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn summarize_reqwest_error(err: &reqwest::Error, fallback: &str) -> String {
    if err.is_timeout() {
        return "request timed out".to_string();
    }
    if err.is_connect() {
        return "network connection failed".to_string();
    }
    if err.is_request() {
        return "network request could not be sent".to_string();
    }
    if err.is_decode() {
        return "response could not be decoded".to_string();
    }

    let normalized = sanitize_provider_error(err.to_string());
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

fn sanitize_provider_error(error: impl AsRef<str>) -> String {
    let raw = error.as_ref().trim();
    if raw.is_empty() {
        return "unknown error".to_string();
    }

    let lowered = raw.to_ascii_lowercase();
    if lowered.contains("error sending request") || lowered.contains("send request failed") {
        return "network request could not be sent".to_string();
    }
    if lowered.contains("dns")
        || lowered.contains("failed to lookup address information")
        || lowered.contains("name or service not known")
        || lowered.contains("no address associated with hostname")
    {
        return "DNS lookup failed".to_string();
    }
    if lowered.contains("timed out") {
        return "request timed out".to_string();
    }
    if lowered.contains("connection refused") {
        return "connection was refused".to_string();
    }

    let without_url = REQWEST_URL_SUFFIX_RE.replace_all(raw, "").to_string();
    normalize_whitespace(without_url.trim())
}

fn browser_command_error(value: &Value, fallback: &str) -> String {
    sanitize_provider_error(
        value
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or(fallback),
    )
}

fn guess_title(description: &str, url: &str) -> String {
    let trimmed = description.trim();
    if let Some((head, _)) = trimmed.split_once(" - ") {
        let title = head.trim();
        if !title.is_empty() {
            return title.to_string();
        }
    }

    guess_title_from_url(url)
}

fn guess_title_from_url(url: &str) -> String {
    if let Ok(parsed) = Url::parse(url) {
        if let Some(segment) = parsed
            .path_segments()
            .and_then(|segments| segments.filter(|item| !item.is_empty()).last())
        {
            let decoded = urlencoding::decode(segment)
                .map(|value| value.into_owned())
                .unwrap_or_else(|_| segment.to_string());
            let normalized = decoded.replace(['-', '_'], " ");
            let title = normalize_whitespace(&normalized);
            if !title.is_empty() {
                return title;
            }
        }

        if let Some(host) = parsed.host_str() {
            return host.to_string();
        }
    }

    normalize_whitespace(url)
}

fn html_to_text(html: &str) -> String {
    let no_script = RE_SCRIPT_TAG.replace_all(html, " ");
    let no_style = RE_STYLE_TAG.replace_all(&no_script, " ");
    let no_noscript = RE_NOSCRIPT_TAG.replace_all(&no_style, " ");
    let raw = RE_HTML_TAG.replace_all(&no_noscript, " ");
    let decoded = decode_basic_html_entities(raw.as_ref());
    normalize_whitespace(decoded.as_str())
}

fn decode_basic_html_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_multiline_text(input: &str) -> String {
    let mut lines = Vec::new();
    let mut previous_blank = false;

    for raw_line in input.lines() {
        let line = normalize_whitespace(raw_line);
        if line.is_empty() {
            if !previous_blank && !lines.is_empty() {
                lines.push(String::new());
            }
            previous_blank = true;
        } else {
            lines.push(line);
            previous_blank = false;
        }
    }

    lines.join("\n").trim().to_string()
}

fn first_non_empty_owned(candidates: &[&str]) -> String {
    for candidate in candidates {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

fn build_extract_summary(content: &str, max_extract_chars: usize) -> ExtractContentSummary {
    let total_chars = content.chars().count();
    let chunk_chars = max_extract_chars.max(1);
    let total_chunks = if total_chars == 0 {
        0
    } else {
        ((total_chars - 1) / chunk_chars) + 1
    };
    let sample_indexes = build_chunk_sample_indexes(total_chunks);

    let sampled_chunks = sample_indexes
        .into_iter()
        .map(|chunk_index| {
            let char_start = chunk_index * chunk_chars;
            let char_end = (char_start + chunk_chars).min(total_chars);
            let span = char_end.saturating_sub(char_start);
            let preview = truncate_chars(slice_chars(content, char_start, span).as_str(), 280);
            ExtractSummaryChunk {
                index: chunk_index,
                char_start,
                char_end,
                preview,
            }
        })
        .collect();

    ExtractContentSummary {
        strategy: "sampled_chunks".to_string(),
        total_chars,
        chunk_chars,
        total_chunks,
        sampled_chunks,
    }
}

fn build_chunk_sample_indexes(total_chunks: usize) -> Vec<usize> {
    if total_chunks == 0 {
        return Vec::new();
    }
    if total_chunks == 1 {
        return vec![0];
    }

    let mut indexes = vec![0, total_chunks / 2, total_chunks - 1];
    indexes.sort_unstable();
    indexes.dedup();
    indexes
}

fn slice_chars(input: &str, start: usize, len: usize) -> String {
    if len == 0 {
        return String::new();
    }
    input.chars().skip(start).take(len).collect()
}

pub(super) fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        content_quality_score, extract_html_page, extract_main_content_text, html_to_text,
        parse_bing_html_results, parse_duckduckgo_html_results, sanitize_provider_error,
        select_research_extract_urls, should_try_browser_render, SearchHit,
    };
    use scraper::Html;
    use std::collections::HashSet;

    #[test]
    fn html_to_text_removes_script_style_and_decodes_entities() {
        let html = r#"
            <html>
              <head>
                <style>body{color:red}</style>
                <script>window.alert('x')</script>
              </head>
              <body>
                Hello&nbsp;World &amp; Team
              </body>
            </html>
        "#;
        let text = html_to_text(html);
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
        assert!(text.contains("Hello World & Team"));
    }

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
    fn main_content_extraction_prefers_article_over_navigation_noise() {
        let html = r#"
            <html>
              <body>
                <header>
                  <nav>
                    <a href="/docs">Docs</a>
                    <a href="/pricing">Pricing</a>
                  </nav>
                </header>
                <main>
                  <article class="docs-content">
                    <h1>Browser Research</h1>
                    <p>The new workflow combines page inspection with external web research.</p>
                    <p>It also extracts article text directly inside chatos without Firecrawl.</p>
                  </article>
                </main>
                <footer>Footer links and privacy notices.</footer>
              </body>
            </html>
        "#;

        let document = Html::parse_document(html);
        let content = extract_main_content_text(&document);

        assert!(content.contains("Browser Research"));
        assert!(content.contains("external web research"));
        assert!(content.contains("inside chatos without Firecrawl"));
        assert!(!content.contains("Footer links"));
        assert!(!content.contains("Pricing"));
    }

    #[test]
    fn html_extract_uses_json_ld_when_visible_body_is_weak() {
        let html = r#"
            <html>
              <head>
                <title>API Pricing</title>
                <meta name="description" content="Short marketing summary.">
                <script type="application/ld+json">
                  {
                    "@context": "https://schema.org",
                    "@type": "TechArticle",
                    "headline": "API Pricing",
                    "articleBody": "The platform includes a free tier, a growth tier, and an enterprise tier. The growth tier starts at $49 per month and includes higher rate limits."
                  }
                </script>
              </head>
              <body>
                <header>Docs | Pricing | Blog</header>
                <main><div class="hero">Choose a plan</div></main>
              </body>
            </html>
        "#;

        let page = extract_html_page("https://example.com/pricing", html.as_bytes(), 10_000);
        assert_eq!(page.title, "API Pricing");
        assert!(page.content.contains("growth tier starts at $49 per month"));
        assert!(page.content.contains("Short marketing summary."));
    }

    #[test]
    fn browser_render_trigger_detects_app_shell_like_pages() {
        let html = r#"
            <html>
              <body>
                <div id="__next">
                  <header>Docs Pricing Blog Sign in</header>
                  <main><div>Loading...</div></main>
                </div>
                <script src="/static/app.js"></script>
                <script src="/static/vendor.js"></script>
                <script src="/static/runtime.js"></script>
                <script src="/static/chunk-a.js"></script>
                <script src="/static/chunk-b.js"></script>
                <script src="/static/chunk-c.js"></script>
              </body>
            </html>
        "#;

        let page = extract_html_page("https://example.com/app", html.as_bytes(), 10_000);
        assert!(should_try_browser_render(html, &page));
    }

    #[test]
    fn content_quality_prefers_richer_article_text() {
        let weak = "Home Pricing Docs Blog Sign in";
        let strong = "Browser research now runs inside chatos.\n\nIt extracts the rendered article body and keeps key details visible.\n\nThis makes dynamic docs pages much easier to read.";

        assert!(content_quality_score(strong) > content_quality_score(weak));
    }

    #[test]
    fn sanitize_provider_error_removes_reqwest_url_noise() {
        let sanitized = sanitize_provider_error(
            "error sending request for url (https://html.duckduckgo.com/html/?q=foo&kp=-1)",
        );
        assert_eq!(sanitized, "network request could not be sent");
    }

    #[test]
    fn select_research_extract_urls_prefers_article_pages_over_sections() {
        let hits = vec![
            SearchHit {
                url: "https://www.reuters.com/world/".to_string(),
                title: "World".to_string(),
                description: String::new(),
            },
            SearchHit {
                url: "https://www.reuters.com/world/china/".to_string(),
                title: "China".to_string(),
                description: String::new(),
            },
            SearchHit {
                url: "https://example.com/blog/browser-research-inside-chatos".to_string(),
                title: "Browser research inside chatos".to_string(),
                description: String::new(),
            },
            SearchHit {
                url: "https://another.example.com/news/2026/04/17/ship-browser-tooling-update"
                    .to_string(),
                title: "Ship browser tooling update".to_string(),
                description: String::new(),
            },
            SearchHit {
                url: "https://www.reuters.com/world/china/china-factory-output-jumps-2026-04-17/"
                    .to_string(),
                title: "Factory output jumps".to_string(),
                description: String::new(),
            },
        ];

        let selected = select_research_extract_urls(&hits, 3, 5);
        assert_eq!(
            selected,
            vec![
                "https://example.com/blog/browser-research-inside-chatos".to_string(),
                "https://another.example.com/news/2026/04/17/ship-browser-tooling-update"
                    .to_string(),
                "https://www.reuters.com/world/china/china-factory-output-jumps-2026-04-17/"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn select_research_extract_urls_falls_back_when_only_section_pages_exist() {
        let hits = vec![
            SearchHit {
                url: "https://www.reuters.com/world/".to_string(),
                title: "World".to_string(),
                description: String::new(),
            },
            SearchHit {
                url: "https://www.reuters.com/business/".to_string(),
                title: "Business".to_string(),
                description: String::new(),
            },
        ];

        let selected = select_research_extract_urls(&hits, 2, 5);
        assert_eq!(
            selected,
            vec![
                "https://www.reuters.com/world/".to_string(),
                "https://www.reuters.com/business/".to_string(),
            ]
        );
    }
}

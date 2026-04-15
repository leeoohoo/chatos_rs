use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct SearchHit {
    pub url: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct ExtractedPage {
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
pub(super) struct ExtractSummaryChunk {
    pub index: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub preview: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct ExtractContentSummary {
    pub strategy: String,
    pub total_chars: usize,
    pub chunk_chars: usize,
    pub total_chunks: usize,
    pub sampled_chunks: Vec<ExtractSummaryChunk>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct ProviderAttempt {
    pub provider: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct SearchOutcome {
    pub backend: String,
    pub fallback_used: bool,
    pub attempts: Vec<ProviderAttempt>,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct ExtractOutcome {
    pub backend: String,
    pub fallback_used: bool,
    pub attempts: Vec<ProviderAttempt>,
    pub pages: Vec<ExtractedPage>,
}

#[derive(Debug, Clone, Copy)]
enum SearchFallbackProvider {
    DuckDuckGo,
}

#[derive(Debug, Clone, Copy)]
enum ExtractFallbackProvider {
    DirectHttp,
}

type SearchProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<SearchHit>, String>> + Send + 'a>>;
type ExtractProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ExtractedPage>, String>> + Send + 'a>>;

trait SearchProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn search<'a>(
        &'a self,
        client: &'a reqwest::Client,
        query: &'a str,
        limit: usize,
    ) -> SearchProviderFuture<'a>;
}

trait ExtractProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn extract<'a>(
        &'a self,
        client: &'a reqwest::Client,
        urls: &'a [String],
        max_extract_chars: usize,
    ) -> ExtractProviderFuture<'a>;
}

#[derive(Debug, Clone, Copy)]
struct FirecrawlSearchProvider;

impl SearchProvider for FirecrawlSearchProvider {
    fn name(&self) -> &'static str {
        "firecrawl"
    }

    fn search<'a>(
        &'a self,
        client: &'a reqwest::Client,
        query: &'a str,
        limit: usize,
    ) -> SearchProviderFuture<'a> {
        Box::pin(async move { firecrawl_search(client, query, limit).await })
    }
}

#[derive(Debug, Clone, Copy)]
struct DuckDuckGoSearchProvider;

impl SearchProvider for DuckDuckGoSearchProvider {
    fn name(&self) -> &'static str {
        "duckduckgo"
    }

    fn search<'a>(
        &'a self,
        client: &'a reqwest::Client,
        query: &'a str,
        limit: usize,
    ) -> SearchProviderFuture<'a> {
        Box::pin(async move { duckduckgo_search(client, query, limit).await })
    }
}

#[derive(Debug, Clone, Copy)]
struct FirecrawlExtractProvider;

impl ExtractProvider for FirecrawlExtractProvider {
    fn name(&self) -> &'static str {
        "firecrawl"
    }

    fn extract<'a>(
        &'a self,
        client: &'a reqwest::Client,
        urls: &'a [String],
        max_extract_chars: usize,
    ) -> ExtractProviderFuture<'a> {
        Box::pin(async move { firecrawl_extract(client, urls, max_extract_chars).await })
    }
}

#[derive(Debug, Clone, Copy)]
struct DirectHttpExtractProvider;

impl ExtractProvider for DirectHttpExtractProvider {
    fn name(&self) -> &'static str {
        "direct_http"
    }

    fn extract<'a>(
        &'a self,
        client: &'a reqwest::Client,
        urls: &'a [String],
        max_extract_chars: usize,
    ) -> ExtractProviderFuture<'a> {
        Box::pin(async move { direct_http_extract(client, urls, max_extract_chars).await })
    }
}

fn build_search_fallback_provider(provider: SearchFallbackProvider) -> Box<dyn SearchProvider> {
    match provider {
        SearchFallbackProvider::DuckDuckGo => Box::new(DuckDuckGoSearchProvider),
    }
}

fn build_extract_fallback_provider(provider: ExtractFallbackProvider) -> Box<dyn ExtractProvider> {
    match provider {
        ExtractFallbackProvider::DirectHttp => Box::new(DirectHttpExtractProvider),
    }
}

static RE_SCRIPT_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("script regex"));
static RE_STYLE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("style regex"));
static RE_HTML_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<[^>]+>").expect("html tag regex"));
static RE_TITLE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").expect("title regex"));

pub(super) async fn search_with_fallback(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<SearchOutcome, String> {
    let fallback = parse_search_fallback_provider()?;
    let mut providers: Vec<Box<dyn SearchProvider>> = Vec::new();
    if firecrawl_api_key().is_ok() {
        providers.push(Box::new(FirecrawlSearchProvider));
    }
    if let Some(provider) = fallback {
        providers.push(build_search_fallback_provider(provider));
    }
    if providers.is_empty() {
        return Err(web_search_configuration_error()
            .unwrap_or_else(|| "web_search is unavailable: no provider configured".to_string()));
    }

    let mut attempts = Vec::new();

    for (index, provider) in providers.iter().enumerate() {
        match provider.search(client, query, limit).await {
            Ok(hits) => {
                return Ok(SearchOutcome {
                    backend: provider.name().to_string(),
                    fallback_used: index > 0,
                    attempts,
                    hits,
                });
            }
            Err(err) => {
                attempts.push(ProviderAttempt {
                    provider: provider.name().to_string(),
                    error: err,
                });
            }
        }
    }

    Err(format!(
        "web_search failed across providers: {}",
        summarize_attempts(&attempts)
    ))
}

pub(super) async fn extract_with_fallback(
    client: &reqwest::Client,
    urls: &[String],
    max_extract_chars: usize,
) -> Result<ExtractOutcome, String> {
    let fallback = parse_extract_fallback_provider()?;
    let mut providers: Vec<Box<dyn ExtractProvider>> = Vec::new();
    if firecrawl_api_key().is_ok() {
        providers.push(Box::new(FirecrawlExtractProvider));
    }
    if let Some(provider) = fallback {
        providers.push(build_extract_fallback_provider(provider));
    }
    if providers.is_empty() {
        return Err(web_extract_configuration_error()
            .unwrap_or_else(|| "web_extract is unavailable: no provider configured".to_string()));
    }

    let mut attempts = Vec::new();

    for (index, provider) in providers.iter().enumerate() {
        match provider.extract(client, urls, max_extract_chars).await {
            Ok(pages) => {
                return Ok(ExtractOutcome {
                    backend: provider.name().to_string(),
                    fallback_used: index > 0,
                    attempts,
                    pages,
                });
            }
            Err(err) => {
                attempts.push(ProviderAttempt {
                    provider: provider.name().to_string(),
                    error: err,
                });
            }
        }
    }

    Err(format!(
        "web_extract failed across providers: {}",
        summarize_attempts(&attempts)
    ))
}

pub(super) async fn firecrawl_search(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let api_key = firecrawl_api_key()?;
    let base_url = firecrawl_base_url();
    let request_body = serde_json::json!({
        "query": query,
        "limit": limit,
    });

    let mut last_error = String::new();
    for path in ["/v1/search", "/v2/search"] {
        let url = format!("{}{}", base_url, path);
        match post_json(client, &url, &api_key, &request_body).await {
            Ok(value) => {
                let hits = parse_search_hits(&value);
                return Ok(hits);
            }
            Err(err) => {
                last_error = err;
                if !last_error.contains("status=404") {
                    break;
                }
            }
        }
    }

    Err(format!("web_search failed: {}", last_error))
}

pub(super) async fn firecrawl_extract(
    client: &reqwest::Client,
    urls: &[String],
    max_extract_chars: usize,
) -> Result<Vec<ExtractedPage>, String> {
    let api_key = firecrawl_api_key()?;
    let base_url = firecrawl_base_url();
    let mut out = Vec::new();

    for url in urls {
        let request_body = serde_json::json!({
            "url": url,
            "formats": ["markdown", "html"]
        });

        let mut last_error = String::new();
        let mut parsed: Option<ExtractedPage> = None;
        for path in ["/v1/scrape", "/v2/scrape"] {
            let endpoint = format!("{}{}", base_url, path);
            match post_json(client, &endpoint, &api_key, &request_body).await {
                Ok(value) => {
                    parsed = Some(parse_scrape_page(url, &value, max_extract_chars));
                    break;
                }
                Err(err) => {
                    last_error = err;
                    if !last_error.contains("status=404") {
                        break;
                    }
                }
            }
        }

        if let Some(page) = parsed {
            out.push(page);
        } else {
            out.push(ExtractedPage {
                url: url.to_string(),
                title: String::new(),
                content: String::new(),
                content_chars: 0,
                original_content_chars: 0,
                truncated: false,
                content_summary: None,
                error: Some(format!("extract failed: {}", last_error)),
            });
        }
    }

    Ok(out)
}

async fn duckduckgo_search(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let response = client
        .get("https://api.duckduckgo.com/")
        .query(&[
            ("q", query),
            ("format", "json"),
            ("no_redirect", "1"),
            ("no_html", "1"),
            ("skip_disambig", "1"),
        ])
        .send()
        .await
        .map_err(|err| format!("request failed: {}", err))?;

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

async fn direct_http_extract(
    client: &reqwest::Client,
    urls: &[String],
    max_extract_chars: usize,
) -> Result<Vec<ExtractedPage>, String> {
    let mut pages = Vec::new();

    for source_url in urls {
        let response = match client.get(source_url).send().await {
            Ok(response) => response,
            Err(err) => {
                pages.push(ExtractedPage {
                    url: source_url.to_string(),
                    title: String::new(),
                    content: String::new(),
                    content_chars: 0,
                    original_content_chars: 0,
                    truncated: false,
                    content_summary: None,
                    error: Some(format!("request failed: {}", err)),
                });
                continue;
            }
        };

        let status = response.status();
        let final_url = response.url().to_string();
        if !status.is_success() {
            pages.push(ExtractedPage {
                url: final_url,
                title: String::new(),
                content: String::new(),
                content_chars: 0,
                original_content_chars: 0,
                truncated: false,
                content_summary: None,
                error: Some(format!("status={}", status)),
            });
            continue;
        }

        let body = match response.text().await {
            Ok(body) => body,
            Err(err) => {
                pages.push(ExtractedPage {
                    url: final_url,
                    title: String::new(),
                    content: String::new(),
                    content_chars: 0,
                    original_content_chars: 0,
                    truncated: false,
                    content_summary: None,
                    error: Some(format!("read response failed: {}", err)),
                });
                continue;
            }
        };

        let title = extract_html_title(body.as_str());
        let mut content = html_to_text(body.as_str());
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

        pages.push(ExtractedPage {
            url: final_url,
            title,
            content,
            content_chars,
            original_content_chars,
            truncated,
            content_summary,
            error: None,
        });
    }

    Ok(pages)
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    payload: &Value,
) -> Result<Value, String> {
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(payload)
        .send()
        .await
        .map_err(|err| format!("request failed: {}", err))?;

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

    serde_json::from_str::<Value>(&text).map_err(|err| {
        format!(
            "invalid JSON response: {} body={}",
            err,
            truncate_chars(&text, 800)
        )
    })
}

fn parse_search_hits(value: &Value) -> Vec<SearchHit> {
    let items = value
        .get("data")
        .and_then(|v| v.as_array())
        .or_else(|| value.get("results").and_then(|v| v.as_array()))
        .or_else(|| value.get("web").and_then(|v| v.as_array()))
        .or_else(|| {
            value
                .get("data")
                .and_then(|v| v.get("web"))
                .and_then(|v| v.as_array())
        });

    let mut out = Vec::new();
    let Some(items) = items else {
        return out;
    };
    for item in items {
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("link").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim()
            .to_string();
        if url.is_empty() {
            continue;
        }
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .or_else(|| {
                item.get("metadata")
                    .and_then(|v| v.get("title"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("")
            .to_string();
        let description = item
            .get("description")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("snippet").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        out.push(SearchHit {
            url,
            title,
            description,
        });
    }
    out
}

fn parse_scrape_page(source_url: &str, value: &Value, max_extract_chars: usize) -> ExtractedPage {
    let payload = value.get("data").unwrap_or(value);

    let metadata = payload
        .get("metadata")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let title = metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let final_url = metadata
        .get("sourceURL")
        .and_then(|v| v.as_str())
        .unwrap_or(source_url)
        .to_string();

    let mut content = payload
        .get("markdown")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("content").and_then(|v| v.as_str()))
        .or_else(|| payload.get("html").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let original_content_chars = content.chars().count();
    let truncated = original_content_chars > max_extract_chars;
    let content_summary = if truncated {
        Some(build_extract_summary(content.as_str(), max_extract_chars))
    } else {
        None
    };
    if truncated {
        content = truncate_chars(&content, max_extract_chars);
    }
    let content_chars = content.chars().count();

    ExtractedPage {
        url: final_url,
        title,
        content,
        content_chars,
        original_content_chars,
        truncated,
        content_summary,
        error: None,
    }
}

fn firecrawl_base_url() -> String {
    std::env::var("FIRECRAWL_API_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://api.firecrawl.dev".to_string())
}

fn firecrawl_api_key() -> Result<String, String> {
    let key = std::env::var("FIRECRAWL_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    key.ok_or_else(|| {
        "FIRECRAWL_API_KEY is not set. Configure it to enable Firecrawl web tools.".to_string()
    })
}

fn parse_search_fallback_provider() -> Result<Option<SearchFallbackProvider>, String> {
    let Some(raw) = std::env::var("WEB_TOOLS_SEARCH_FALLBACK")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    match raw.as_str() {
        "duckduckgo" | "ddg" => Ok(Some(SearchFallbackProvider::DuckDuckGo)),
        _ => Err(format!(
            "Unsupported WEB_TOOLS_SEARCH_FALLBACK='{}'. Supported: duckduckgo",
            raw
        )),
    }
}

fn parse_extract_fallback_provider() -> Result<Option<ExtractFallbackProvider>, String> {
    let Some(raw) = std::env::var("WEB_TOOLS_EXTRACT_FALLBACK")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    match raw.as_str() {
        "direct_http" | "http" | "native" => Ok(Some(ExtractFallbackProvider::DirectHttp)),
        _ => Err(format!(
            "Unsupported WEB_TOOLS_EXTRACT_FALLBACK='{}'. Supported: direct_http",
            raw
        )),
    }
}

pub(super) fn web_search_configuration_error() -> Option<String> {
    if firecrawl_api_key().is_ok() {
        return None;
    }

    match parse_search_fallback_provider() {
        Ok(Some(_)) => None,
        Ok(None) => Some(
            "FIRECRAWL_API_KEY is not set. Configure it, or set WEB_TOOLS_SEARCH_FALLBACK=duckduckgo for fallback search."
                .to_string(),
        ),
        Err(err) => Some(err),
    }
}

pub(super) fn web_extract_configuration_error() -> Option<String> {
    if firecrawl_api_key().is_ok() {
        return None;
    }

    match parse_extract_fallback_provider() {
        Ok(Some(_)) => None,
        Ok(None) => Some(
            "FIRECRAWL_API_KEY is not set. Configure it, or set WEB_TOOLS_EXTRACT_FALLBACK=direct_http for fallback extract."
                .to_string(),
        ),
        Err(err) => Some(err),
    }
}

fn summarize_attempts(attempts: &[ProviderAttempt]) -> String {
    if attempts.is_empty() {
        return "no provider attempted".to_string();
    }
    attempts
        .iter()
        .map(|item| format!("{}: {}", item.provider, item.error))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn guess_title(description: &str, url: &str) -> String {
    let trimmed = description.trim();
    if let Some((head, _)) = trimmed.split_once(" - ") {
        let title = head.trim();
        if !title.is_empty() {
            return title.to_string();
        }
    }

    let from_url = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim();
    if !from_url.is_empty() {
        return from_url.to_string();
    }

    trimmed.to_string()
}

fn extract_html_title(html: &str) -> String {
    let Some(captures) = RE_TITLE_TAG.captures(html) else {
        return String::new();
    };
    let Some(group) = captures.get(1) else {
        return String::new();
    };
    normalize_whitespace(decode_basic_html_entities(group.as_str()).as_str())
}

fn html_to_text(html: &str) -> String {
    let no_script = RE_SCRIPT_TAG.replace_all(html, " ");
    let no_style = RE_STYLE_TAG.replace_all(&no_script, " ");
    let raw = RE_HTML_TAG.replace_all(&no_style, " ");
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
    use std::sync::Mutex;

    use once_cell::sync::Lazy;
    use serde_json::json;

    use super::{
        html_to_text, parse_scrape_page, web_extract_configuration_error,
        web_search_configuration_error,
    };

    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn search_config_error_depends_on_fallback_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let prev_key = std::env::var("FIRECRAWL_API_KEY").ok();
        let prev_fallback = std::env::var("WEB_TOOLS_SEARCH_FALLBACK").ok();

        std::env::remove_var("FIRECRAWL_API_KEY");
        std::env::remove_var("WEB_TOOLS_SEARCH_FALLBACK");
        let missing = web_search_configuration_error()
            .expect("search should be unavailable without key and fallback");
        assert!(missing.contains("WEB_TOOLS_SEARCH_FALLBACK=duckduckgo"));

        std::env::set_var("WEB_TOOLS_SEARCH_FALLBACK", "duckduckgo");
        assert!(web_search_configuration_error().is_none());

        std::env::set_var("WEB_TOOLS_SEARCH_FALLBACK", "unknown_provider");
        let invalid = web_search_configuration_error()
            .expect("invalid fallback should produce configuration error");
        assert!(invalid.contains("Unsupported WEB_TOOLS_SEARCH_FALLBACK"));

        restore_env("FIRECRAWL_API_KEY", prev_key);
        restore_env("WEB_TOOLS_SEARCH_FALLBACK", prev_fallback);
    }

    #[test]
    fn extract_config_error_depends_on_fallback_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let prev_key = std::env::var("FIRECRAWL_API_KEY").ok();
        let prev_fallback = std::env::var("WEB_TOOLS_EXTRACT_FALLBACK").ok();

        std::env::remove_var("FIRECRAWL_API_KEY");
        std::env::remove_var("WEB_TOOLS_EXTRACT_FALLBACK");
        let missing = web_extract_configuration_error()
            .expect("extract should be unavailable without key and fallback");
        assert!(missing.contains("WEB_TOOLS_EXTRACT_FALLBACK=direct_http"));

        std::env::set_var("WEB_TOOLS_EXTRACT_FALLBACK", "direct_http");
        assert!(web_extract_configuration_error().is_none());

        std::env::set_var("WEB_TOOLS_EXTRACT_FALLBACK", "unknown_provider");
        let invalid = web_extract_configuration_error()
            .expect("invalid fallback should produce configuration error");
        assert!(invalid.contains("Unsupported WEB_TOOLS_EXTRACT_FALLBACK"));

        restore_env("FIRECRAWL_API_KEY", prev_key);
        restore_env("WEB_TOOLS_EXTRACT_FALLBACK", prev_fallback);
    }

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
    fn parse_scrape_page_sets_truncation_summary_for_large_content() {
        let long_text = (0..400)
            .map(|idx| format!("Segment-{idx}"))
            .collect::<Vec<_>>()
            .join(" ");
        let payload = json!({
            "data": {
                "metadata": {
                    "title": "Long Page",
                    "sourceURL": "https://example.com/long"
                },
                "markdown": long_text
            }
        });

        let page = parse_scrape_page("https://example.com/source", &payload, 120);
        assert_eq!(page.url, "https://example.com/long");
        assert!(page.truncated);
        assert!(page.original_content_chars > page.content_chars);
        let summary = page
            .content_summary
            .expect("summary required for truncated page");
        assert_eq!(summary.strategy, "sampled_chunks");
        assert!(summary.total_chunks >= 2);
        assert!(!summary.sampled_chunks.is_empty());
    }

    #[test]
    fn parse_scrape_page_keeps_summary_empty_for_small_content() {
        let payload = json!({
            "data": {
                "metadata": {
                    "title": "Short Page",
                    "sourceURL": "https://example.com/short"
                },
                "markdown": "hello world"
            }
        });

        let page = parse_scrape_page("https://example.com/source", &payload, 120);
        assert!(!page.truncated);
        assert_eq!(page.original_content_chars, page.content_chars);
        assert!(page.content_summary.is_none());
    }

    fn restore_env(key: &str, value: Option<String>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}

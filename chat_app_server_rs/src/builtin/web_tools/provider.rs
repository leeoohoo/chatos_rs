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
    pub error: Option<String>,
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
                error: Some(format!("extract failed: {}", last_error)),
            });
        }
    }

    Ok(out)
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

    if content.chars().count() > max_extract_chars {
        content = truncate_chars(&content, max_extract_chars);
    }

    ExtractedPage {
        url: final_url,
        title,
        content,
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
        "FIRECRAWL_API_KEY is not set. Configure it to enable web_search/web_extract.".to_string()
    })
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

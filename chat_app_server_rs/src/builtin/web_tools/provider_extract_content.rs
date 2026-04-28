use serde_json::Value;

use super::super::provider_types::{ExtractedPage, ResponseContentKind};
use super::super::provider_utils::{build_extract_summary, guess_title_from_url, normalize_multiline_text};

pub(super) fn detect_response_content_kind(
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

pub(super) fn extract_json_page(
    final_url: &str,
    body: &[u8],
    max_extract_chars: usize,
) -> ExtractedPage {
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

pub(super) fn extract_text_page(
    final_url: &str,
    body: &[u8],
    max_extract_chars: usize,
) -> ExtractedPage {
    let raw = String::from_utf8_lossy(body).to_string();
    let content = if raw.contains('<') && raw.contains('>') {
        super::html_to_text(&raw)
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

pub(super) fn extract_pdf_page(
    final_url: &str,
    body: &[u8],
    max_extract_chars: usize,
) -> ExtractedPage {
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

pub(super) fn finalize_page(
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
        content = super::super::provider_utils::truncate_chars(content.as_str(), max_extract_chars);
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

pub(super) fn error_page(requested_url: String, final_url: String, error: String) -> ExtractedPage {
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

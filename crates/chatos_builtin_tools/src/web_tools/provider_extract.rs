// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "provider_extract_browser.rs"]
mod provider_extract_browser;
#[path = "provider_extract_content.rs"]
mod provider_extract_content;
#[path = "provider_extract_html.rs"]
mod provider_extract_html;
#[path = "provider_extract_support.rs"]
mod provider_extract_support;

use crate::browser_runtime::browser_backend_available;

use super::provider_types::{BrowserRenderOptions, ExtractedPage, ResponseContentKind};

use self::provider_extract_browser::extract_html_with_optional_browser_render;
use self::provider_extract_content::error_page;
use self::provider_extract_content::{
    detect_response_content_kind, extract_json_page, extract_pdf_page, extract_text_page,
};
use self::provider_extract_support::fetch_extract_response;

#[cfg(test)]
pub(super) fn content_quality_score(content: &str) -> usize {
    provider_extract_support::content_quality_score(content)
}

#[cfg(test)]
pub(super) fn extract_main_content_text(document: &scraper::Html) -> String {
    provider_extract_html::extract_main_content_text(document)
}

pub(super) fn html_to_text(html: &str) -> String {
    provider_extract_html::html_to_text(html)
}

pub(crate) async fn native_extract(
    client: &reqwest::Client,
    urls: &[String],
    max_extract_chars: usize,
    browser_options: Option<&BrowserRenderOptions>,
) -> Result<Vec<ExtractedPage>, String> {
    let mut pages = Vec::new();
    let browser_render_available =
        browser_options.is_some_and(|_| browser_backend_available().is_ok());

    for raw_source_url in urls {
        let fetched = match fetch_extract_response(
            client,
            raw_source_url.as_str(),
            max_extract_chars,
        )
        .await
        {
            Ok(fetched) => fetched,
            Err(page) => {
                pages.push(page);
                continue;
            }
        };

        let kind = detect_response_content_kind(
            fetched.content_type.as_deref(),
            &fetched.source_url,
            &fetched.body,
        );
        let page = match kind {
            ResponseContentKind::Html => {
                extract_html_with_optional_browser_render(
                    fetched.final_url.as_str(),
                    fetched.body.as_slice(),
                    max_extract_chars,
                    browser_render_available,
                    browser_options,
                )
                .await
            }
            ResponseContentKind::Json => extract_json_page(
                &fetched.final_url,
                fetched.body.as_slice(),
                max_extract_chars,
            ),
            ResponseContentKind::Text => extract_text_page(
                &fetched.final_url,
                fetched.body.as_slice(),
                max_extract_chars,
            ),
            ResponseContentKind::Pdf => extract_pdf_page(
                &fetched.final_url,
                fetched.body.as_slice(),
                max_extract_chars,
            ),
            ResponseContentKind::Unsupported(kind) => error_page(
                fetched.source_url,
                fetched.final_url,
                format!("unsupported content type: {}", kind),
            ),
        };
        pages.push(page);
    }

    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::provider_extract_browser::should_try_browser_render;
    use super::provider_extract_html::extract_html_page;
    use super::{content_quality_score, extract_main_content_text, html_to_text};
    use scraper::Html;

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
}

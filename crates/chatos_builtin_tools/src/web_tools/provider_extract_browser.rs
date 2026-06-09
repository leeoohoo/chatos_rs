use serde_json::Value;

use super::super::provider_browser_support::{
    close_browser_page, eval_on_browser_page, is_browser_command_success, open_browser_page,
    parse_browser_eval_result, snapshot_browser_page,
};
use super::super::provider_types::{BrowserRenderOptions, BrowserRenderedPage, ExtractedPage};
use super::super::provider_utils::{
    first_non_empty_owned, js_string_array, js_string_literal, normalize_multiline_text,
    normalized_text_key, text_contains_any_marker, truncate_chars,
};
use super::provider_extract_content::finalize_page;
use super::provider_extract_html::extract_html_page;
use super::provider_extract_support::{
    content_quality_score, count_text_marker_hits, merge_content_candidates, non_empty_line_count,
    sentenceish_count, BLOCK_SELECTOR, CONTENT_HINT_MARKERS, CONTENT_SELECTOR_LIST,
    MIN_BROWSER_RENDER_TRIGGER_CHARS, NAVIGATION_MARKERS, NOISE_ATTR_MARKERS,
    NOISE_EXCLUSION_MARKERS, NOISE_TAGS, SPA_SHELL_MARKERS, TABLE_CELL_SELECTOR,
    WEAK_RENDER_TRIGGER_MARKERS,
};
const BROWSER_RENDER_OPEN_TIMEOUT_SECONDS: u64 = 60;

pub(super) async fn extract_html_with_optional_browser_render(
    final_url: &str,
    body: &[u8],
    max_extract_chars: usize,
    browser_render_available: bool,
    browser_options: Option<&BrowserRenderOptions>,
) -> ExtractedPage {
    let static_page = extract_html_page(final_url, body, max_extract_chars);
    if !(browser_render_available
        && should_try_browser_render(&String::from_utf8_lossy(body), &static_page))
    {
        return static_page;
    }

    let Some(options) = browser_options else {
        return static_page;
    };

    match browser_render_extract(final_url, max_extract_chars, options).await {
        Ok(Some(rendered)) => maybe_upgrade_html_page(static_page, rendered, max_extract_chars),
        Ok(None) | Err(_) => static_page,
    }
}

pub(super) async fn browser_render_extract(
    url: &str,
    max_extract_chars: usize,
    options: &BrowserRenderOptions,
) -> Result<Option<BrowserRenderedPage>, String> {
    let Some(session) =
        open_browser_page(url, options, BROWSER_RENDER_OPEN_TIMEOUT_SECONDS).await?
    else {
        return Ok(None);
    };

    let snapshot_result = snapshot_browser_page(&session, true, options).await?;
    let snapshot = snapshot_result
        .get("data")
        .and_then(|value| value.get("snapshot"))
        .and_then(|value| value.as_str())
        .map(|value| truncate_chars(value, max_extract_chars))
        .unwrap_or_default();

    let eval_result = eval_on_browser_page(
        &session,
        browser_render_eval_expression(),
        options,
        options.command_timeout_seconds,
    )
    .await?;
    close_browser_page(&session, options).await;
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

pub(super) fn maybe_upgrade_html_page(
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

pub(super) fn should_try_browser_render(raw_html: &str, page: &ExtractedPage) -> bool {
    let content = page.content.trim();
    if content.is_empty() {
        return true;
    }

    let normalized = normalized_text_key(content);
    if normalized.is_empty() {
        return true;
    }

    if text_contains_any_marker(&normalized, WEAK_RENDER_TRIGGER_MARKERS) {
        return true;
    }

    let chars = content.chars().count();
    if chars < MIN_BROWSER_RENDER_TRIGGER_CHARS {
        return true;
    }

    let non_empty_lines = non_empty_line_count(content);
    let sentenceish = sentenceish_count(&normalized);
    let navigation_hits = count_text_marker_hits(&normalized, NAVIGATION_MARKERS);
    let raw_lower = raw_html.to_ascii_lowercase();
    let has_spa_shell = text_contains_any_marker(&raw_lower, SPA_SHELL_MARKERS);
    let script_count = raw_lower.matches("<script").count();
    let looks_sparse = non_empty_lines <= 4 && sentenceish <= 2;
    let looks_navigation_heavy = chars < 700 && navigation_hits >= 4 && sentenceish <= 2;

    (has_spa_shell || script_count >= 6) && (looks_sparse || looks_navigation_heavy)
}

fn browser_render_eval_expression() -> String {
    let template = r##"JSON.stringify((() => {
  const selectorList = __SELECTOR_LIST__;
  const blockSelector = __BLOCK_SELECTOR__;
  const tableCellSelector = __TABLE_CELL_SELECTOR__;
  const noiseTags = __NOISE_TAGS__;
  const positiveAttrMarkers = __POSITIVE_ATTR_MARKERS__;
  const noiseAttrMarkers = __NOISE_ATTR_MARKERS__;
  const contentHintMarkers = __CONTENT_HINT_MARKERS__;
  const clean = (text) => (text || "").replace(/\u00a0/g, " ").replace(/\s+/g, " ").trim();
  const attrsContain = (value, markers) => markers.some((marker) => value.includes(marker));
  const isNoise = (element) => {
    if (!element) return false;
    const tag = (element.tagName || "").toLowerCase();
    if (noiseTags.includes(tag)) {
      return true;
    }
    const attrs = [
      element.id || "",
      element.className || "",
      element.getAttribute("role") || "",
      element.getAttribute("aria-label") || "",
      element.getAttribute("data-testid") || ""
    ].join(" ").toLowerCase();
    if (attrsContain(attrs, positiveAttrMarkers)) {
      return false;
    }
    return attrsContain(attrs, noiseAttrMarkers);
  };
  const formatBlock = (element) => {
    const tag = (element.tagName || "").toLowerCase();
    if (tag === "tr") {
      const cells = Array.from(element.querySelectorAll(tableCellSelector))
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
    ].join(" ").toLowerCase();
    const attrBonus = attrsContain(attrs, contentHintMarkers) ? 180 : 0;
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
})())"##;

    template
        .replace("__SELECTOR_LIST__", &js_string_array(CONTENT_SELECTOR_LIST))
        .replace("__BLOCK_SELECTOR__", &js_string_literal(BLOCK_SELECTOR))
        .replace(
            "__TABLE_CELL_SELECTOR__",
            &js_string_literal(TABLE_CELL_SELECTOR),
        )
        .replace("__NOISE_TAGS__", &js_string_array(NOISE_TAGS))
        .replace(
            "__POSITIVE_ATTR_MARKERS__",
            &js_string_array(NOISE_EXCLUSION_MARKERS),
        )
        .replace(
            "__NOISE_ATTR_MARKERS__",
            &js_string_array(NOISE_ATTR_MARKERS),
        )
        .replace(
            "__CONTENT_HINT_MARKERS__",
            &js_string_array(CONTENT_HINT_MARKERS),
        )
}

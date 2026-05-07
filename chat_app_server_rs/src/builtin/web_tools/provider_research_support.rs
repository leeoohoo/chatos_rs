use std::collections::HashSet;

use url::Url;

use super::provider_types::{ResearchUrlCandidate, SearchHit};
use super::provider_utils::normalize_public_web_url;

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
        for candidate in &candidates {
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
    let url = normalize_public_web_url(raw_url)?;

    let parsed = Url::parse(url.as_str()).ok();
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
        url,
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

#[cfg(test)]
mod tests {
    use super::select_research_extract_urls;
    use crate::builtin::web_tools::provider::SearchHit;

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

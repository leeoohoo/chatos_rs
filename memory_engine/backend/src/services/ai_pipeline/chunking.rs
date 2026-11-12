// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;

use sha2::{Digest, Sha256};

const CHUNK_DIGEST_PREFIX: &str = "sha256:";
const SECTION_MARKERS: [&str; 4] = [
    "[tool_calls]",
    "[tool_result]",
    "[structured_payload]",
    "[reasoning]",
];

pub fn estimate_tokens_text(text: &str) -> i64 {
    let char_count = text.chars().count() as i64;
    let byte_count = text.len() as i64;
    let extra_bytes = byte_count.saturating_sub(char_count);
    ((char_count * 3 + extra_bytes * 4 + 11) / 12).max(1)
}

pub(crate) fn split_chunks_by_token_limit(
    items: &[String],
    token_limit: i64,
    split_oversized_items: bool,
) -> Vec<Vec<String>> {
    if items.is_empty() {
        return Vec::new();
    }

    let normalized = if split_oversized_items {
        items
            .iter()
            .flat_map(|item| split_text_by_token_limit(item.as_str(), token_limit))
            .collect::<Vec<_>>()
    } else {
        items.to_vec()
    };

    let mut queue: VecDeque<Vec<String>> = VecDeque::new();
    let mut leaves = Vec::new();
    queue.push_back(normalized);

    while let Some(chunk) = queue.pop_front() {
        if chunk.is_empty() {
            continue;
        }

        let chunk_tokens = chunk
            .iter()
            .map(|item| estimate_tokens_text(item.as_str()))
            .sum::<i64>();
        if chunk_tokens > token_limit && chunk.len() > 1 {
            let mid = chunk.len() / 2;
            queue.push_back(chunk[..mid].to_vec());
            queue.push_back(chunk[mid..].to_vec());
            continue;
        }

        leaves.push(chunk);
    }

    leaves
}

pub(super) fn contains_source_ref(item: &str, source_ref: &str) -> bool {
    if source_ref.trim().is_empty() {
        return false;
    }
    let marker_match = ["record_id", "summary_id", "memory_id", "source_ref", "id"]
        .iter()
        .any(|key| item.contains(format!("[{key}={source_ref}]").as_str()))
        || item.contains(format!("[source_ref={source_ref}#chunk:").as_str());
    marker_match
        || serde_json::from_str::<serde_json::Value>(item.trim())
            .is_ok_and(|value| json_contains_source_ref(&value, source_ref))
}

fn json_contains_source_ref(value: &serde_json::Value, source_ref: &str) -> bool {
    match value {
        serde_json::Value::Object(object) => {
            object.get("source_refs").is_some_and(|refs| {
                refs.as_array()
                    .is_some_and(|refs| refs.iter().any(|value| value.as_str() == Some(source_ref)))
            }) || object
                .values()
                .any(|value| json_contains_source_ref(value, source_ref))
        }
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| json_contains_source_ref(value, source_ref)),
        _ => false,
    }
}

pub(super) fn split_text_by_token_limit(text: &str, token_limit: i64) -> Vec<String> {
    if estimate_tokens_text(text) <= token_limit {
        return vec![text.to_string()];
    }

    let identity = source_identity(text);
    let source_ref = identity
        .as_ref()
        .map(|identity| identity.value.clone())
        .unwrap_or_else(|| format!("content:{}", hex::encode(Sha256::digest(text.as_bytes()))));
    let identity_marker = identity
        .map(|identity| format!("[{}={}]", identity.key, identity.value))
        .unwrap_or_default();
    let envelope_probe = format!(
        "[source_ref={source_ref}#chunk:999999/999999][chunk_digest={CHUNK_DIGEST_PREFIX}{}]\n{identity_marker}\n",
        "0".repeat(64)
    );
    let body_limit = token_limit
        .saturating_sub(estimate_tokens_text(envelope_probe.as_str()))
        .max(1);
    let semantic = semantic_fragments(text);
    let mut atoms = Vec::new();
    for fragment in semantic {
        atoms.extend(split_fragment_to_fit(fragment.as_str(), body_limit));
    }
    let mut bodies = pack_fragments(atoms, body_limit);
    if bodies.is_empty() {
        bodies = split_plain_text_to_fit(text, body_limit);
    }

    let count = bodies.len();
    let mut chunks = Vec::with_capacity(count);
    for (index, body) in bodies.into_iter().enumerate() {
        let digest = format!(
            "{CHUNK_DIGEST_PREFIX}{}",
            hex::encode(Sha256::digest(body.as_bytes()))
        );
        let identity_line = if identity_marker.is_empty() {
            String::new()
        } else {
            format!("{identity_marker}\n")
        };
        chunks.push(format!(
            "[source_ref={source_ref}#chunk:{}/{}][chunk_digest={digest}]\n{identity_line}{body}",
            index + 1,
            count
        ));
    }
    chunks
}

#[derive(Debug, Clone)]
struct SourceIdentity {
    key: String,
    value: String,
}

fn source_identity(text: &str) -> Option<SourceIdentity> {
    const KEYS: [&str; 5] = ["record_id", "summary_id", "memory_id", "source_ref", "id"];
    for key in KEYS {
        let prefix = format!("[{key}=");
        let Some(start) = text.find(prefix.as_str()) else {
            continue;
        };
        let value_start = start + prefix.len();
        let Some(relative_end) = text[value_start..].find(']') else {
            continue;
        };
        let value = text[value_start..value_start + relative_end].trim();
        if !value.is_empty() {
            return Some(SourceIdentity {
                key: key.to_string(),
                value: value.to_string(),
            });
        }
    }
    None
}

fn semantic_fragments(text: &str) -> Vec<String> {
    let body = text
        .split_once('\n')
        .filter(|(first, _)| source_identity(first).is_some())
        .map(|(_, body)| body)
        .unwrap_or(text);
    let sections = split_sections(body);
    let mut fragments = Vec::new();
    for (section, content) in sections {
        let trimmed = content.trim_matches(['\r', '\n']);
        if trimmed.is_empty() || section == "reasoning" {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let mut leaves = Vec::new();
            flatten_json(&json, "$", &mut leaves);
            for (path, value) in leaves {
                fragments.push(format!("[section={section}][json_path={path}]\n{value}"));
            }
            continue;
        }
        for part in paragraph_and_line_fragments(trimmed) {
            fragments.push(format!("[section={section}]\n{part}"));
        }
    }
    fragments
}

fn split_sections(text: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut label = "text".to_string();
    let mut content = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(marker) = SECTION_MARKERS.iter().find(|marker| **marker == trimmed) {
            if !content.trim().is_empty() {
                sections.push((label, content.trim_matches(['\r', '\n']).to_string()));
            }
            label = marker
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();
            content.clear();
            continue;
        }
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(line);
    }
    if !content.trim().is_empty() {
        sections.push((label, content.trim_matches(['\r', '\n']).to_string()));
    }
    sections
}

fn flatten_json(value: &serde_json::Value, path: &str, output: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::Object(object) if !object.is_empty() => {
            for (key, value) in object {
                let encoded_key =
                    serde_json::to_string(key).unwrap_or_else(|_| "\"?\"".to_string());
                flatten_json(value, format!("{path}[{encoded_key}]").as_str(), output);
            }
        }
        serde_json::Value::Array(values) if !values.is_empty() => {
            for (index, value) in values.iter().enumerate() {
                flatten_json(value, format!("{path}[{index}]").as_str(), output);
            }
        }
        value => output.push((
            path.to_string(),
            serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
        )),
    }
}

fn paragraph_and_line_fragments(text: &str) -> Vec<String> {
    let mut fragments = Vec::new();
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim_matches(['\r', '\n']);
        if paragraph.trim().is_empty() {
            continue;
        }
        if paragraph.contains('\n') {
            fragments.extend(
                paragraph
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(str::to_string),
            );
        } else {
            fragments.push(paragraph.to_string());
        }
    }
    fragments
}

fn split_fragment_to_fit(fragment: &str, token_limit: i64) -> Vec<String> {
    if estimate_tokens_text(fragment) <= token_limit {
        return vec![fragment.to_string()];
    }
    let (prefix, content) = fragment
        .split_once('\n')
        .map(|(prefix, content)| (format!("{prefix}\n"), content))
        .unwrap_or_else(|| (String::new(), fragment));
    let content_limit = token_limit
        .saturating_sub(estimate_tokens_text(prefix.as_str()))
        .max(1);
    split_plain_text_to_fit(content, content_limit)
        .into_iter()
        .map(|part| format!("{prefix}{part}"))
        .collect()
}

fn split_plain_text_to_fit(text: &str, token_limit: i64) -> Vec<String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut parts = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let mut low = start + 1;
        let mut high = chars.len();
        let mut best = start + 1;
        while low <= high {
            let mid = low + (high - low) / 2;
            let candidate = chars[start..mid].iter().collect::<String>();
            if estimate_tokens_text(candidate.as_str()) <= token_limit {
                best = mid;
                low = mid + 1;
            } else {
                high = mid.saturating_sub(1);
            }
        }
        let part = chars[start..best].iter().collect::<String>();
        if !part.trim().is_empty() {
            parts.push(part);
        }
        start = best;
    }
    parts
}

fn pack_fragments(fragments: Vec<String>, token_limit: i64) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for fragment in fragments {
        let candidate = if current.is_empty() {
            fragment.clone()
        } else {
            format!("{current}\n\n{fragment}")
        };
        if !current.is_empty() && estimate_tokens_text(candidate.as_str()) > token_limit {
            chunks.push(current);
            current = fragment;
        } else {
            current = candidate;
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

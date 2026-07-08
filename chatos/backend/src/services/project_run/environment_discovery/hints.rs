// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;

use super::super::file_limits::{read_to_string_limited, MAX_MANIFEST_BYTES};

#[derive(Debug, Default)]
pub(super) struct ProjectToolchainHints {
    pub(super) tokens_by_kind: HashMap<String, Vec<String>>,
}

fn read_trimmed_file(path: &Path) -> Option<String> {
    read_to_string_limited(path, MAX_MANIFEST_BYTES)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

fn extract_numeric_fragments(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn hint_variants(kind: &str, raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    if trimmed.is_empty() {
        return out;
    }

    let normalized = trimmed.to_lowercase();
    out.push(normalized.clone());

    if kind == "node" && normalized.starts_with('v') && normalized.len() > 1 {
        out.push(normalized[1..].to_string());
    }
    if kind == "go" && normalized.starts_with("go") && normalized.len() > 2 {
        out.push(normalized[2..].to_string());
    }

    for fragment in extract_numeric_fragments(normalized.as_str()) {
        if !fragment.is_empty() {
            out.push(fragment);
        }
    }

    out.sort();
    out.dedup();
    out
}

fn push_hint(tokens_by_kind: &mut HashMap<String, Vec<String>>, kind: &str, raw: &str) {
    let entry = tokens_by_kind.entry(kind.to_string()).or_default();
    entry.extend(hint_variants(kind, raw));
    entry.sort();
    entry.dedup();
}

fn parse_tool_versions(content: &str, tokens_by_kind: &mut HashMap<String, Vec<String>>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(tool) = parts.next() else {
            continue;
        };
        let Some(version) = parts.next() else {
            continue;
        };
        match tool {
            "java" => push_hint(tokens_by_kind, "java_home", version),
            "maven" => push_hint(tokens_by_kind, "mvn", version),
            "gradle" => push_hint(tokens_by_kind, "gradle", version),
            "rust" => {
                push_hint(tokens_by_kind, "cargo", version);
                push_hint(tokens_by_kind, "rustc", version);
            }
            "golang" => push_hint(tokens_by_kind, "go", version),
            "nodejs" => push_hint(tokens_by_kind, "node", version),
            "python" => push_hint(tokens_by_kind, "python", version),
            _ => {}
        }
    }
}

fn parse_sdkmanrc(content: &str, tokens_by_kind: &mut HashMap<String, Vec<String>>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((tool, version)) = trimmed.split_once('=') else {
            continue;
        };
        let normalized_tool = tool.trim();
        let normalized_version = version.trim();
        if normalized_tool.is_empty() || normalized_version.is_empty() {
            continue;
        }
        match normalized_tool {
            "java" => push_hint(tokens_by_kind, "java_home", normalized_version),
            "maven" => push_hint(tokens_by_kind, "mvn", normalized_version),
            "gradle" => push_hint(tokens_by_kind, "gradle", normalized_version),
            _ => {}
        }
    }
}

fn parse_go_hint(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("toolchain ") {
            if !value.trim().is_empty() {
                return Some(value.trim().to_string());
            }
        }
    }
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("go ") {
            if !value.trim().is_empty() {
                return Some(format!("go{}", value.trim()));
            }
        }
    }
    None
}

fn parse_rust_toolchain_hint(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.starts_with('[') {
        for line in trimmed.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("channel") {
                let Some((_, value)) = rest.split_once('=') else {
                    continue;
                };
                let channel = value.trim().trim_matches('"').trim_matches('\'');
                if !channel.is_empty() {
                    return Some(channel.to_string());
                }
            }
        }
        return None;
    }
    first_non_empty_line(trimmed)
}

pub(super) fn collect_project_toolchain_hints(project_root: &Path) -> ProjectToolchainHints {
    let mut tokens_by_kind = HashMap::<String, Vec<String>>::new();

    if let Some(value) = read_trimmed_file(project_root.join(".nvmrc").as_path()) {
        push_hint(&mut tokens_by_kind, "node", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".node-version").as_path()) {
        push_hint(&mut tokens_by_kind, "node", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".python-version").as_path()) {
        push_hint(&mut tokens_by_kind, "python", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".java-version").as_path()) {
        push_hint(&mut tokens_by_kind, "java_home", value.as_str());
    }
    if let Some(value) = read_trimmed_file(project_root.join(".tool-versions").as_path()) {
        parse_tool_versions(value.as_str(), &mut tokens_by_kind);
    }
    if let Some(value) = read_trimmed_file(project_root.join(".sdkmanrc").as_path()) {
        parse_sdkmanrc(value.as_str(), &mut tokens_by_kind);
    }
    if let Some(value) = read_trimmed_file(project_root.join("rust-toolchain").as_path()) {
        if let Some(hint) = parse_rust_toolchain_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "cargo", hint.as_str());
            push_hint(&mut tokens_by_kind, "rustc", hint.as_str());
        }
    }
    if let Some(value) = read_trimmed_file(project_root.join("rust-toolchain.toml").as_path()) {
        if let Some(hint) = parse_rust_toolchain_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "cargo", hint.as_str());
            push_hint(&mut tokens_by_kind, "rustc", hint.as_str());
        }
    }
    if let Some(value) = read_trimmed_file(project_root.join("go.mod").as_path()) {
        if let Some(hint) = parse_go_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "go", hint.as_str());
        }
    }
    if let Some(value) = read_trimmed_file(project_root.join("go.work").as_path()) {
        if let Some(hint) = parse_go_hint(value.as_str()) {
            push_hint(&mut tokens_by_kind, "go", hint.as_str());
        }
    }

    ProjectToolchainHints { tokens_by_kind }
}

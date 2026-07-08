// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::services::code_nav::file_limits::read_code_nav_file_to_string;
use crate::services::code_nav::languages::regex_utils::compile_static_regex;
use crate::services::code_nav::languages::shared_nav::{
    count_char, declaration_kind_from_symbol_kind as shared_declaration_kind_from_symbol_kind,
    find_column,
};
use crate::services::code_nav::types::NavLocation;

use super::search::RustSearchMatch;
use super::RustSymbol;

static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(r"\b(struct|enum|trait|type|mod)\s+([A-Za-z_][A-Za-z0-9_]*)")
});
static FN_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    )
});
static CONST_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    )
});
static LET_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*let(?:\s+mut)?\s+([A-Za-z_][A-Za-z0-9_]*)\b"));
static IMPL_RE: Lazy<Regex> = Lazy::new(|| compile_static_regex(r"^\s*impl\b.*"));

#[derive(Debug, Clone)]
pub(super) struct RustFileAnalysis {
    pub(super) symbols: Vec<RustSymbol>,
}

#[derive(Debug, Clone)]
struct BlockScope {
    kind: String,
    body_depth: i32,
}

pub(super) fn analyze_rust_file(path: &Path) -> Result<RustFileAnalysis, String> {
    let content = read_code_nav_file_to_string(path)?;
    let mut symbols = Vec::new();
    let mut block_scopes: Vec<BlockScope> = Vec::new();
    let mut brace_depth: i32 = 0;
    let mut in_block_comment = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let sanitized = strip_rust_comments(raw_line, &mut in_block_comment);
        let trimmed = sanitized.trim();
        if trimmed.is_empty() {
            brace_depth += count_char(&sanitized, '{') as i32;
            brace_depth -= count_char(&sanitized, '}') as i32;
            pop_block_scopes(&mut block_scopes, brace_depth);
            continue;
        }

        if let Some(capture) = TYPE_RE.captures(trimmed) {
            let kind = match capture.get(1).map(|item| item.as_str()) {
                Some("struct") => "struct",
                Some("enum") => "enum",
                Some("trait") => "trait",
                Some("type") => "type",
                Some("mod") => "module",
                _ => "type",
            };
            let name = capture[2].to_string();
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            symbols.push(RustSymbol {
                name: name.clone(),
                kind: kind.to_string(),
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
            if trimmed.contains('{') && matches!(kind, "trait") {
                block_scopes.push(BlockScope {
                    kind: "trait".to_string(),
                    body_depth: brace_depth + 1,
                });
            }
        }

        if IMPL_RE.is_match(trimmed) && trimmed.contains('{') {
            block_scopes.push(BlockScope {
                kind: "impl".to_string(),
                body_depth: brace_depth + 1,
            });
        }

        if let Some(capture) = FN_RE.captures(trimmed) {
            let name = capture[1].to_string();
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            let kind = if block_scopes
                .last()
                .map(|scope| {
                    scope.body_depth == brace_depth
                        && matches!(scope.kind.as_str(), "impl" | "trait")
                })
                .unwrap_or(false)
            {
                "method"
            } else {
                "function"
            };
            symbols.push(RustSymbol {
                name,
                kind: kind.to_string(),
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
        } else if let Some(capture) = CONST_RE.captures(trimmed) {
            let name = capture[1].to_string();
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            symbols.push(RustSymbol {
                name,
                kind: "constant".to_string(),
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
        }

        brace_depth += count_char(&sanitized, '{') as i32;
        brace_depth -= count_char(&sanitized, '}') as i32;
        pop_block_scopes(&mut block_scopes, brace_depth);
    }

    symbols.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.name.cmp(&right.name))
    });

    Ok(RustFileAnalysis { symbols })
}

pub(super) fn resolve_rust_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<RustFileAnalysis>>,
    entry: &RustSearchMatch,
    token: &str,
) -> Option<&'static str> {
    resolve_rust_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_rust_declaration(&entry.text, token))
}

pub(super) fn is_rust_declaration_location(
    analysis_cache: &mut HashMap<String, Option<RustFileAnalysis>>,
    location: &NavLocation,
    token: &str,
) -> bool {
    let entry = RustSearchMatch {
        path: location.path.clone(),
        relative_path: location.relative_path.clone(),
        line: location.line,
        column: location.column,
        text: location.preview.clone(),
    };
    resolve_rust_declaration_kind(analysis_cache, &entry, token).is_some()
}

fn resolve_rust_symbol_kind(
    analysis_cache: &mut HashMap<String, Option<RustFileAnalysis>>,
    entry: &RustSearchMatch,
    token: &str,
) -> Option<&'static str> {
    let analysis = cached_rust_analysis(analysis_cache, &entry.path)?;
    let symbol = analysis
        .symbols
        .iter()
        .find(|item| item.name == token && item.line == entry.line && item.column == entry.column)
        .or_else(|| {
            analysis
                .symbols
                .iter()
                .find(|item| item.name == token && item.line == entry.line)
        })?;
    declaration_kind_from_symbol_kind(symbol.kind.as_str())
}

fn cached_rust_analysis<'a>(
    analysis_cache: &'a mut HashMap<String, Option<RustFileAnalysis>>,
    path: &str,
) -> Option<&'a RustFileAnalysis> {
    if !analysis_cache.contains_key(path) {
        analysis_cache.insert(path.to_string(), analyze_rust_file(Path::new(path)).ok());
    }
    analysis_cache.get(path).and_then(|item| item.as_ref())
}

fn declaration_kind_from_symbol_kind(kind: &str) -> Option<&'static str> {
    shared_declaration_kind_from_symbol_kind(kind)
}

fn classify_rust_declaration(line: &str, token: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if let Some(capture) = TYPE_RE.captures(trimmed) {
        if capture.get(2).map(|item| item.as_str()) == Some(token) {
            return match capture.get(1).map(|item| item.as_str()) {
                Some("struct") => Some("struct"),
                Some("enum") => Some("enum"),
                Some("trait") => Some("trait"),
                Some("type") => Some("type"),
                Some("mod") => Some("module"),
                _ => Some("type"),
            };
        }
    }
    if let Some(capture) = FN_RE.captures(trimmed) {
        if capture.get(1).map(|item| item.as_str()) == Some(token) {
            return Some("function");
        }
    }
    if let Some(capture) = CONST_RE.captures(trimmed) {
        if capture.get(1).map(|item| item.as_str()) == Some(token) {
            return Some("constant");
        }
    }
    if let Some(capture) = LET_RE.captures(trimmed) {
        if capture.get(1).map(|item| item.as_str()) == Some(token) {
            return Some("variable");
        }
    }
    None
}

fn pop_block_scopes(block_scopes: &mut Vec<BlockScope>, brace_depth: i32) {
    while block_scopes
        .last()
        .map(|scope| brace_depth < scope.body_depth)
        .unwrap_or(false)
    {
        block_scopes.pop();
    }
}

fn strip_rust_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    let mut in_string = false;
    let mut string_delim = '\0';

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if *in_block_comment {
            if current == '*' && next == Some('/') {
                *in_block_comment = false;
                out.push(' ');
                out.push(' ');
                index += 2;
            } else {
                out.push(' ');
                index += 1;
            }
            continue;
        }

        if in_string {
            out.push(current);
            if current == '\\' {
                if let Some(next) = next {
                    out.push(next);
                    index += 2;
                    continue;
                }
            }
            if current == string_delim {
                in_string = false;
                string_delim = '\0';
            }
            index += 1;
            continue;
        }

        if current == '"' || current == '\'' {
            in_string = true;
            string_delim = current;
            out.push(current);
            index += 1;
            continue;
        }

        if current == '/' && next == Some('/') {
            while index < chars.len() {
                out.push(' ');
                index += 1;
            }
            break;
        }

        if current == '/' && next == Some('*') {
            *in_block_comment = true;
            out.push(' ');
            out.push(' ');
            index += 2;
            continue;
        }

        out.push(current);
        index += 1;
    }

    out
}

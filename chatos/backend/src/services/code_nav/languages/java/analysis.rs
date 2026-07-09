// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use once_cell::sync::Lazy;
use regex::Regex;
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::file_limits::read_code_nav_file_to_string;
use crate::services::code_nav::languages::regex_utils::compile_static_regex;
use crate::services::code_nav::languages::shared_nav::{
    count_char, declaration_kind_from_symbol_kind as shared_declaration_kind_from_symbol_kind,
    ensure_code_nav_text_search_budget, find_column, is_type_like, nav_location_from_coordinates,
    normalize_path, search_text_occurrences, TextSearchLine, TextSearchMatchParts,
};
use crate::services::code_nav::types::{NavLocation, NavPositionRequest, ProjectContext};

mod syntax;

use syntax::strip_java_comments;
pub(crate) use syntax::{extract_field_name, extract_method_signature};

pub(crate) const JAVA_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".gradle",
];

pub(crate) const JAVA_EXTENSIONS: &[&str] = &["java"];

static PACKAGE_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*package\s+([A-Za-z_][A-Za-z0-9_.]*)\s*;"));
static IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(r"^\s*import\s+(static\s+)?([A-Za-z_][A-Za-z0-9_.]*(?:\.\*)?)\s*;")
});
static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(r"\b(class|interface|enum|record)\s+([A-Za-z_][A-Za-z0-9_]*)")
});

#[derive(Debug, Clone)]
pub(crate) struct JavaImport {
    pub(crate) path: String,
    pub(crate) is_static: bool,
    pub(crate) is_wildcard: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct JavaSymbol {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct JavaFileAnalysis {
    pub(crate) package_name: Option<String>,
    pub(crate) imports: Vec<JavaImport>,
    pub(crate) symbols: Vec<JavaSymbol>,
    pub(crate) primary_type: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct JavaSearchMatch {
    pub(crate) path: String,
    pub(crate) relative_path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) text: String,
}

#[derive(Debug, Clone)]
struct TypeScope {
    name: String,
    body_depth: i32,
}

pub(crate) fn analyze_java_file(path: &Path) -> Result<JavaFileAnalysis, String> {
    let content = read_code_nav_file_to_string(path)?;
    let mut package_name = None;
    let mut imports = Vec::new();
    let mut symbols = Vec::new();
    let mut primary_type = None;
    let mut type_stack: Vec<TypeScope> = Vec::new();
    let mut brace_depth: i32 = 0;
    let mut in_block_comment = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let sanitized = strip_java_comments(raw_line, &mut in_block_comment);
        let trimmed = sanitized.trim();
        if trimmed.is_empty() {
            brace_depth += count_char(&sanitized, '{') as i32;
            brace_depth -= count_char(&sanitized, '}') as i32;
            pop_type_scopes(&mut type_stack, brace_depth);
            continue;
        }

        if package_name.is_none() {
            if let Some(capture) = PACKAGE_RE.captures(trimmed) {
                package_name = Some(capture[1].to_string());
            }
        }

        if let Some(capture) = IMPORT_RE.captures(trimmed) {
            let path = capture[2].to_string();
            imports.push(JavaImport {
                is_static: capture.get(1).is_some(),
                is_wildcard: path.ends_with(".*"),
                path,
            });
        }

        if let Some(capture) = TYPE_RE.captures(trimmed) {
            let kind = capture[1].to_string();
            let name = capture[2].to_string();
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            if primary_type.is_none() {
                primary_type = Some(name.clone());
            }
            symbols.push(JavaSymbol {
                name: name.clone(),
                kind,
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
            type_stack.push(TypeScope {
                name,
                body_depth: brace_depth + 1,
            });
        }

        if let Some(current_type) = type_stack.last() {
            if brace_depth == current_type.body_depth {
                if let Some((method_name, method_kind)) =
                    extract_method_signature(trimmed, current_type.name.as_str())
                {
                    let column = find_column(raw_line, &method_name).unwrap_or(1);
                    let end_column = column + method_name.chars().count().saturating_sub(1);
                    symbols.push(JavaSymbol {
                        name: method_name,
                        kind: method_kind,
                        line: line_number,
                        column,
                        end_line: line_number,
                        end_column,
                    });
                } else if let Some(field_name) = extract_field_name(trimmed) {
                    let column = find_column(raw_line, &field_name).unwrap_or(1);
                    let end_column = column + field_name.chars().count().saturating_sub(1);
                    symbols.push(JavaSymbol {
                        name: field_name,
                        kind: "field".to_string(),
                        line: line_number,
                        column,
                        end_line: line_number,
                        end_column,
                    });
                }
            }
        }

        brace_depth += count_char(&sanitized, '{') as i32;
        brace_depth -= count_char(&sanitized, '}') as i32;
        pop_type_scopes(&mut type_stack, brace_depth);
    }

    symbols.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.name.cmp(&right.name))
    });

    Ok(JavaFileAnalysis {
        package_name,
        imports,
        symbols,
        primary_type,
    })
}

pub(crate) fn resolve_imported_type_paths(
    root: &Path,
    analysis: &JavaFileAnalysis,
    token: &str,
) -> Result<Vec<PathBuf>, String> {
    if !is_type_like(token) {
        return Ok(Vec::new());
    }

    let mut packages = Vec::new();
    if let Some(package_name) = &analysis.package_name {
        packages.push(package_name.clone());
    }
    for item in &analysis.imports {
        if item.is_static {
            continue;
        }
        if item.is_wildcard {
            packages.push(item.path.trim_end_matches(".*").to_string());
        } else if item.path.rsplit('.').next() == Some(token) {
            let package_name = item
                .path
                .rsplit_once('.')
                .map(|value| value.0.to_string())
                .unwrap_or_default();
            packages.push(package_name);
        }
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for package_name in packages {
        for path in resolve_type_file_by_package(root, &package_name, token)? {
            let key = path.to_string_lossy().to_string();
            if seen.insert(key) {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn resolve_type_file_by_package(
    root: &Path,
    package_name: &str,
    token: &str,
) -> Result<Vec<PathBuf>, String> {
    let relative = if package_name.is_empty() {
        PathBuf::from(format!("{token}.java"))
    } else {
        PathBuf::from(package_name.replace('.', "/")).join(format!("{token}.java"))
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for base in java_source_roots(root) {
        let candidate = base.join(&relative);
        if candidate.exists() {
            let normalized = normalize_path(&candidate);
            let key = normalized.to_string_lossy().to_string();
            if seen.insert(key) {
                out.push(normalized);
            }
        }
    }

    if !out.is_empty() {
        return Ok(out);
    }

    let target_name = format!("{token}.java");
    let started_at = Instant::now();
    let mut visited_entries = 0usize;
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_java_path(entry))
    {
        visited_entries = visited_entries.saturating_add(1);
        ensure_code_nav_text_search_budget(started_at, visited_entries)?;

        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_string_lossy() != target_name {
            continue;
        }
        let normalized = normalize_path(entry.path());
        let analysis = analyze_java_file(&normalized)?;
        if analysis.package_name.as_deref().unwrap_or("") == package_name {
            let key = normalized.to_string_lossy().to_string();
            if seen.insert(key) {
                out.push(normalized);
            }
        }
    }

    Ok(out)
}

pub(crate) fn search_java_occurrences(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
) -> Result<Vec<JavaSearchMatch>, String> {
    search_text_occurrences(
        root,
        query,
        case_sensitive,
        whole_word,
        max_results,
        JAVA_IGNORED_DIRS,
        |path| path.extension().and_then(|value| value.to_str()) == Some("java"),
        |_path, content| content.lines().map(TextSearchLine::plain).collect(),
        |parts: TextSearchMatchParts| JavaSearchMatch {
            path: parts.path,
            relative_path: parts.relative_path,
            line: parts.line,
            column: parts.column,
            text: parts.text,
        },
    )
}

pub(crate) fn nav_location_from_symbol(
    root: &Path,
    path: &Path,
    symbol: &JavaSymbol,
    score: f64,
) -> Result<Option<NavLocation>, String> {
    nav_location_from_coordinates(
        root,
        path,
        symbol.line,
        symbol.column,
        symbol.end_line,
        symbol.end_column,
        score,
    )
}

pub(crate) fn score_java_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    declaration_kind: &str,
    entry: &JavaSearchMatch,
    resolved_type_paths: &HashSet<String>,
) -> f64 {
    let mut score = 0.0;
    let is_same_file = entry.relative_path == ctx.relative_path;
    let is_same_line = is_same_file && entry.line == req.line;
    let file_stem = Path::new(&entry.relative_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if resolved_type_paths.contains(&entry.path) {
        score += 10.0;
    }
    if file_stem == token {
        score += 5.0;
    }
    if is_same_file {
        score += 2.0;
    }
    if is_same_line {
        score -= 5.0;
    }

    score += match declaration_kind {
        "class" | "interface" | "enum" | "record" => 7.0,
        "constructor" => 6.0,
        "method" => 5.0,
        "field" => 4.0,
        _ => 1.0,
    };

    if is_type_like(token) && is_type_symbol(declaration_kind) {
        score += 2.0;
    }

    score
}

pub(crate) fn resolve_java_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<JavaFileAnalysis>>,
    entry: &JavaSearchMatch,
    token: &str,
    current_type_name: Option<&str>,
) -> Option<&'static str> {
    resolve_java_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_java_declaration(&entry.text, token, current_type_name))
}

pub(crate) fn is_java_declaration_location(
    analysis_cache: &mut HashMap<String, Option<JavaFileAnalysis>>,
    location: &NavLocation,
    token: &str,
    current_type_name: Option<&str>,
) -> bool {
    let entry = JavaSearchMatch {
        path: location.path.clone(),
        relative_path: location.relative_path.clone(),
        line: location.line,
        column: location.column,
        text: location.preview.clone(),
    };
    resolve_java_declaration_kind(analysis_cache, &entry, token, current_type_name).is_some()
}

fn resolve_java_symbol_kind(
    analysis_cache: &mut HashMap<String, Option<JavaFileAnalysis>>,
    entry: &JavaSearchMatch,
    token: &str,
) -> Option<&'static str> {
    let analysis = cached_java_analysis(analysis_cache, &entry.path)?;
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

fn cached_java_analysis<'a>(
    analysis_cache: &'a mut HashMap<String, Option<JavaFileAnalysis>>,
    path: &str,
) -> Option<&'a JavaFileAnalysis> {
    if !analysis_cache.contains_key(path) {
        analysis_cache.insert(path.to_string(), analyze_java_file(Path::new(path)).ok());
    }
    analysis_cache.get(path).and_then(|item| item.as_ref())
}

fn declaration_kind_from_symbol_kind(kind: &str) -> Option<&'static str> {
    shared_declaration_kind_from_symbol_kind(kind)
}

pub(crate) fn classify_java_declaration(
    line: &str,
    token: &str,
    current_type_name: Option<&str>,
) -> Option<&'static str> {
    let trimmed = line.trim();
    if let Some(capture) = TYPE_RE.captures(trimmed) {
        if capture.get(2).map(|value| value.as_str()) == Some(token) {
            return match capture.get(1).map(|value| value.as_str()) {
                Some("class") => Some("class"),
                Some("interface") => Some("interface"),
                Some("enum") => Some("enum"),
                Some("record") => Some("record"),
                _ => Some("type"),
            };
        }
    }

    if let Some((name, kind)) = extract_method_signature(trimmed, current_type_name.unwrap_or("")) {
        if name == token {
            return Some(if kind == "constructor" {
                "constructor"
            } else {
                "method"
            });
        }
    }

    if let Some(field_name) = extract_field_name(trimmed) {
        if field_name == token {
            return Some("field");
        }
    }

    None
}

fn is_type_symbol(kind: &str) -> bool {
    matches!(kind, "class" | "interface" | "enum" | "record" | "type")
}

fn should_visit_java_path(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !JAVA_IGNORED_DIRS.contains(&name)
}

fn java_source_roots(root: &Path) -> Vec<PathBuf> {
    let candidates = [
        root.join("src/main/java"),
        root.join("src/test/java"),
        root.join("src/integrationTest/java"),
        root.join("src/androidTest/java"),
        root.join("app/src/main/java"),
        root.to_path_buf(),
    ];

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        if candidate.exists() {
            let normalized = normalize_path(&candidate);
            let key = normalized.to_string_lossy().to_string();
            if seen.insert(key) {
                out.push(normalized);
            }
        }
    }
    out
}

fn pop_type_scopes(type_stack: &mut Vec<TypeScope>, brace_depth: i32) {
    while type_stack
        .last()
        .map(|item| brace_depth < item.body_depth)
        .unwrap_or(false)
    {
        type_stack.pop();
    }
}

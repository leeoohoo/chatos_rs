use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::file_limits::{read_code_nav_file_to_string, truncate_preview};
use crate::services::code_nav::languages::regex_utils::compile_static_regex;
use crate::services::code_nav::languages::shared_nav::{
    declaration_kind_from_symbol_kind as shared_declaration_kind_from_symbol_kind,
    ensure_code_nav_text_search_budget, find_column, is_type_like, nav_location_from_coordinates,
    normalize_path,
};
use crate::services::code_nav::types::{NavLocation, NavPositionRequest, ProjectContext};

mod syntax;

use syntax::{
    classify_go_declaration, extract_go_function_name, extract_go_method_name,
    extract_go_top_level_binding, extract_go_type_declaration, parse_go_import_block_item,
    parse_go_single_import, strip_go_comments,
};

pub(crate) const GO_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    "vendor",
];

pub(crate) const GO_EXTENSIONS: &[&str] = &["go"];

static GO_MODULE_RE: Lazy<Regex> = Lazy::new(|| compile_static_regex(r"^\s*module\s+([^\s]+)\s*$"));

#[derive(Debug, Clone)]
pub(crate) struct GoImport {
    pub(crate) path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GoSymbol {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct GoFileAnalysis {
    pub(crate) imports: Vec<GoImport>,
    pub(crate) symbols: Vec<GoSymbol>,
}

#[derive(Debug, Clone)]
pub(crate) struct GoSearchMatch {
    pub(crate) path: String,
    pub(crate) relative_path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) text: String,
}

pub(crate) fn analyze_go_file(path: &Path) -> Result<GoFileAnalysis, String> {
    let content = read_code_nav_file_to_string(path)?;
    let mut imports = Vec::new();
    let mut symbols = Vec::new();
    let mut in_block_comment = false;
    let mut in_import_block = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let sanitized = strip_go_comments(raw_line, &mut in_block_comment);
        let trimmed = sanitized.trim();
        if trimmed.is_empty() {
            continue;
        }

        if in_import_block {
            if trimmed.starts_with(')') {
                in_import_block = false;
                continue;
            }
            if let Some(import_item) = parse_go_import_block_item(trimmed) {
                imports.push(import_item);
            }
            continue;
        }

        if trimmed == "import(" || trimmed == "import (" || trimmed.starts_with("import (") {
            in_import_block = true;
            continue;
        }

        if let Some(import_item) = parse_go_single_import(trimmed) {
            imports.push(import_item);
            continue;
        }

        if let Some((name, kind)) = extract_go_type_declaration(trimmed) {
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            symbols.push(GoSymbol {
                name,
                kind,
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
            continue;
        }

        if let Some(name) = extract_go_method_name(trimmed) {
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            symbols.push(GoSymbol {
                name,
                kind: "method".to_string(),
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
            continue;
        }

        if let Some(name) = extract_go_function_name(trimmed) {
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            symbols.push(GoSymbol {
                name,
                kind: "function".to_string(),
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
            continue;
        }

        if let Some((name, kind)) = extract_go_top_level_binding(trimmed) {
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            symbols.push(GoSymbol {
                name,
                kind,
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
        }
    }

    symbols.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.name.cmp(&right.name))
    });

    Ok(GoFileAnalysis { imports, symbols })
}

pub(crate) fn resolve_imported_symbol_files(
    root: &Path,
    analysis: &GoFileAnalysis,
    token: &str,
) -> Result<Vec<PathBuf>, String> {
    let Some(module_path) = go_module_path(root)? else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for import_item in &analysis.imports {
        let Some(package_dir) = resolve_go_import_dir(root, &module_path, &import_item.path) else {
            continue;
        };
        for path in go_package_files(&package_dir)? {
            let file_analysis = analyze_go_file(&path)?;
            if !file_analysis.symbols.iter().any(|item| item.name == token) {
                continue;
            }
            let key = path.to_string_lossy().to_string();
            if seen.insert(key) {
                out.push(path);
            }
        }
    }

    Ok(out)
}

pub(crate) fn search_go_occurrences(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
) -> Result<Vec<GoSearchMatch>, String> {
    let pattern = if whole_word {
        format!(r"\b{}\b", regex::escape(query))
    } else {
        regex::escape(query)
    };
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .unicode(true)
        .build()
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    let started_at = Instant::now();
    let mut visited_entries = 0usize;

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_go_path(entry))
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
        if entry.path().extension().and_then(|value| value.to_str()) != Some("go") {
            continue;
        }
        let content = match read_code_nav_file_to_string(entry.path()) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let mut in_block_comment = false;
        for (index, raw_line) in content.lines().enumerate() {
            if index % 128 == 0 {
                ensure_code_nav_text_search_budget(started_at, visited_entries)?;
            }

            let sanitized = strip_go_comments(raw_line, &mut in_block_comment);
            let normalized = sanitized.trim_end_matches('\r');
            for found in regex.find_iter(normalized) {
                if out.len() >= max_results {
                    return Ok(out);
                }
                let column = normalized[..found.start()].chars().count() + 1;
                let relative_path = pathdiff::diff_paths(entry.path(), root)
                    .unwrap_or_else(|| entry.path().to_path_buf())
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(GoSearchMatch {
                    path: normalize_path(entry.path()).to_string_lossy().to_string(),
                    relative_path,
                    line: index + 1,
                    column,
                    text: truncate_preview(raw_line.trim_end_matches('\r'), 400),
                });
            }
        }
    }

    Ok(out)
}

pub(crate) fn nav_location_from_symbol(
    root: &Path,
    path: &Path,
    symbol: &GoSymbol,
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

pub(crate) fn score_go_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    declaration_kind: &str,
    entry: &GoSearchMatch,
    resolved_path_set: &HashSet<String>,
) -> f64 {
    let mut score = 0.0;
    let is_same_file = entry.relative_path == ctx.relative_path;
    let is_same_line = is_same_file && entry.line == req.line;
    let file_stem = Path::new(&entry.relative_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if resolved_path_set.contains(&entry.path) {
        score += 10.0;
    }
    if file_stem == token {
        score += 4.0;
    }
    if is_same_file {
        score += 2.0;
    }
    if is_same_line {
        score -= 4.0;
    }

    score += match declaration_kind {
        "struct" | "interface" | "type" => 7.0,
        "method" => 6.0,
        "function" => 5.0,
        "constant" => 4.0,
        "variable" => 3.0,
        _ => 1.0,
    };

    if is_type_like(token) && is_type_symbol(declaration_kind) {
        score += 2.0;
    }

    score
}

pub(crate) fn resolve_go_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<GoFileAnalysis>>,
    entry: &GoSearchMatch,
    token: &str,
) -> Option<&'static str> {
    resolve_go_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_go_declaration(&entry.text, token))
}

pub(crate) fn is_go_declaration_location(
    analysis_cache: &mut HashMap<String, Option<GoFileAnalysis>>,
    location: &NavLocation,
    token: &str,
) -> bool {
    let entry = GoSearchMatch {
        path: location.path.clone(),
        relative_path: location.relative_path.clone(),
        line: location.line,
        column: location.column,
        text: location.preview.clone(),
    };
    resolve_go_declaration_kind(analysis_cache, &entry, token).is_some()
}

fn go_module_path(root: &Path) -> Result<Option<String>, String> {
    let path = root.join("go.mod");
    if !path.exists() {
        return Ok(None);
    }
    let content = read_code_nav_file_to_string(&path)?;
    for line in content.lines() {
        if let Some(capture) = GO_MODULE_RE.captures(line) {
            return Ok(Some(capture[1].to_string()));
        }
    }
    Ok(None)
}

fn resolve_go_import_dir(root: &Path, module_path: &str, import_path: &str) -> Option<PathBuf> {
    let relative = if import_path == module_path {
        PathBuf::new()
    } else if let Some(stripped) = import_path.strip_prefix(&format!("{module_path}/")) {
        PathBuf::from(stripped)
    } else {
        return None;
    };

    let candidate = normalize_path(&root.join(relative));
    if candidate.exists() && candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn go_package_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let started_at = Instant::now();
    let mut visited_entries = 0usize;
    for entry in WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_entry(|entry| should_visit_go_path(entry))
    {
        visited_entries = visited_entries.saturating_add(1);
        ensure_code_nav_text_search_budget(started_at, visited_entries)?;

        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => return Err(err.to_string()),
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("go") {
            continue;
        }
        out.push(normalize_path(entry.path()));
    }
    Ok(out)
}

fn resolve_go_symbol_kind(
    analysis_cache: &mut HashMap<String, Option<GoFileAnalysis>>,
    entry: &GoSearchMatch,
    token: &str,
) -> Option<&'static str> {
    let analysis = cached_go_analysis(analysis_cache, &entry.path)?;
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

fn cached_go_analysis<'a>(
    analysis_cache: &'a mut HashMap<String, Option<GoFileAnalysis>>,
    path: &str,
) -> Option<&'a GoFileAnalysis> {
    if !analysis_cache.contains_key(path) {
        analysis_cache.insert(path.to_string(), analyze_go_file(Path::new(path)).ok());
    }
    analysis_cache.get(path).and_then(|item| item.as_ref())
}

fn declaration_kind_from_symbol_kind(kind: &str) -> Option<&'static str> {
    shared_declaration_kind_from_symbol_kind(kind)
}

fn is_type_symbol(kind: &str) -> bool {
    matches!(kind, "struct" | "interface" | "type")
}

fn should_visit_go_path(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !GO_IGNORED_DIRS.contains(&name)
}

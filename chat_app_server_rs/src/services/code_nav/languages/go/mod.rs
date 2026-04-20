use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::symbol_index::{
    nav_location_from_indexed_symbol, project_symbol_index, score_indexed_definition_candidate,
    IndexedSymbol,
};
use crate::services::code_nav::types::{
    DocumentSymbolItem, DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities,
    NavLocation, NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;

const GO_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    "vendor",
];

const GO_EXTENSIONS: &[&str] = &["go"];
const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

static GO_MODULE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*module\s+([^\s]+)\s*$").unwrap());
static IMPORT_SINGLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^\s*import\s+(?:(?:([A-Za-z_][A-Za-z0-9_]*)|_|\.)\s+)?"([^"]+)""#).unwrap()
});
static IMPORT_BLOCK_ITEM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*(?:(?:([A-Za-z_][A-Za-z0-9_]*)|_|\.)\s+)?"([^"]+)""#).unwrap());
static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)\s+(struct|interface)\b").unwrap()
});
static TYPE_ALIAS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());
static METHOD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*func\s*\([^)]*\)\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap());
static FUNCTION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*func\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap());
static VAR_CONST_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(var|const)\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());
static SHORT_VAR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:=").unwrap());

#[derive(Debug, Clone)]
struct GoImport {
    path: String,
}

#[derive(Debug, Clone)]
struct GoSymbol {
    name: String,
    kind: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Clone)]
struct GoFileAnalysis {
    imports: Vec<GoImport>,
    symbols: Vec<GoSymbol>,
}

#[derive(Debug, Clone)]
struct GoSearchMatch {
    path: String,
    relative_path: String,
    line: usize,
    column: usize,
    text: String,
}

fn indexed_go_symbols(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_go_file(path)?;
    Ok(analysis
        .symbols
        .into_iter()
        .map(|symbol| IndexedSymbol {
            name: symbol.name,
            kind: symbol.kind,
            line: symbol.line,
            column: symbol.column,
            end_line: symbol.end_line,
            end_column: symbol.end_column,
        })
        .collect())
}

#[derive(Default)]
pub struct GoCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for GoCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "go"
    }

    fn language_id(&self) -> &'static str {
        "go"
    }

    fn definition_mode(&self) -> &'static str {
        "provider-heuristic"
    }

    fn references_mode(&self) -> &'static str {
        "provider-heuristic"
    }

    fn document_symbols_mode(&self) -> &'static str {
        "provider-heuristic"
    }

    fn supports_file(&self, file_path: &Path) -> bool {
        file_path.extension().and_then(|value| value.to_str()) == Some("go")
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("go.mod").exists()
    }

    fn capabilities(&self, _ctx: &ProjectContext) -> NavCapabilities {
        NavCapabilities {
            supports_definition: true,
            supports_references: true,
            supports_document_symbols: true,
        }
    }

    async fn definition(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        go_definition(ctx, req)
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        go_references(ctx, req)
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = analyze_go_file(&ctx.file_path)?;
        let mut symbols: Vec<DocumentSymbolItem> = analysis
            .symbols
            .into_iter()
            .map(|item| DocumentSymbolItem {
                name: item.name,
                kind: item.kind,
                line: item.line,
                column: item.column,
                end_line: item.end_line,
                end_column: item.end_column,
            })
            .collect();
        if symbols.len() > MAX_SYMBOL_RESULTS {
            symbols.truncate(MAX_SYMBOL_RESULTS);
        }

        Ok(DocumentSymbolsResponse {
            provider: self.provider_id().to_string(),
            language: self.language_id().to_string(),
            mode: self.document_symbols_mode().to_string(),
            symbols,
        })
    }
}

fn go_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_go_file(&ctx.file_path)?;
    let resolved_import_files = resolve_imported_symbol_files(&ctx.root, &current, &token)?;
    let resolved_path_set: HashSet<String> = resolved_import_files
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    for symbol in current
        .symbols
        .iter()
        .filter(|item| item.name == token && item.line != req.line)
    {
        if let Some(location) = nav_location_from_symbol(&ctx.root, &ctx.file_path, symbol, 9.0)? {
            push_unique_location(&mut candidates, &mut seen, location);
        }
    }

    for path in resolved_import_files {
        let analysis = analyze_go_file(&path)?;
        for symbol in analysis.symbols.iter().filter(|item| item.name == token) {
            let score = if is_type_like(&token) && is_type_symbol(&symbol.kind) {
                16.0
            } else {
                12.0
            };
            if let Some(location) = nav_location_from_symbol(&ctx.root, &path, symbol, score)? {
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "go",
        GO_EXTENSIONS,
        GO_IGNORED_DIRS,
        indexed_go_symbols,
    ) {
        if let Some(symbols) = index.symbols_by_name.get(&token) {
            for indexed in symbols {
                if indexed.relative_path == ctx.relative_path && indexed.symbol.line == req.line {
                    continue;
                }
                let mut score = score_indexed_definition_candidate(ctx, req, indexed);
                if resolved_path_set.contains(&indexed.path) {
                    score += 10.0;
                }
                let location = match nav_location_from_indexed_symbol(&ctx.root, indexed, score) {
                    Ok(location) => location,
                    Err(_) => continue,
                };
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if candidates.is_empty() {
        let mut analysis_cache = HashMap::new();
        let mut search_matches =
            search_go_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
        if search_matches.is_empty() {
            search_matches =
                search_go_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
        }

        for entry in search_matches {
            let Some(declaration_kind) =
                resolve_go_declaration_kind(&mut analysis_cache, &entry, &token)
            else {
                continue;
            };
            let score = score_go_definition_candidate(
                ctx,
                req,
                &token,
                declaration_kind,
                &entry,
                &resolved_path_set,
            );
            let location = NavLocation {
                path: entry.path,
                relative_path: entry.relative_path,
                line: entry.line,
                column: entry.column,
                end_line: entry.line,
                end_column: entry.column + token.chars().count().saturating_sub(1),
                preview: entry.text,
                score,
            };
            push_unique_location(&mut candidates, &mut seen, location);
        }
    }

    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    if candidates.len() > MAX_DEFINITION_RESULTS {
        candidates.truncate(MAX_DEFINITION_RESULTS);
    }

    Ok(candidates)
}

fn go_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let mut matches = search_go_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
    if matches.is_empty() {
        matches = search_go_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
    }

    let mut locations = Vec::new();
    let mut seen = HashSet::new();
    for entry in matches {
        let location = NavLocation {
            score: if entry.relative_path == ctx.relative_path {
                1.5
            } else {
                1.0
            },
            path: entry.path,
            relative_path: entry.relative_path,
            line: entry.line,
            column: entry.column,
            end_line: entry.line,
            end_column: entry.column + token.chars().count().saturating_sub(1),
            preview: entry.text,
        };
        let key = build_nav_key(&location);
        if seen.insert(key) {
            locations.push(location);
        }
    }

    let mut declarations = Vec::new();
    let mut references = Vec::new();
    let mut classification_cache = HashMap::new();
    for location in locations {
        if is_go_declaration_location(&mut classification_cache, &location, &token) {
            declarations.push(location);
        } else {
            references.push(location);
        }
    }

    let mut out = if references.is_empty() {
        declarations
    } else {
        references
    };
    out.sort_by(|left, right| {
        (left.relative_path != ctx.relative_path)
            .cmp(&(right.relative_path != ctx.relative_path))
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    if out.len() > MAX_REFERENCE_RESULTS {
        out.truncate(MAX_REFERENCE_RESULTS);
    }

    Ok(out)
}

fn analyze_go_file(path: &Path) -> Result<GoFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
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

fn resolve_imported_symbol_files(
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

fn go_module_path(root: &Path) -> Result<Option<String>, String> {
    let path = root.join("go.mod");
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
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
    for entry in WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_entry(|entry| should_visit_go_path(entry))
    {
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

fn search_go_occurrences(
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

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_go_path(entry))
    {
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
        let content = match fs::read_to_string(entry.path()) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let mut in_block_comment = false;
        for (index, raw_line) in content.lines().enumerate() {
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
                    text: raw_line.trim_end_matches('\r').chars().take(400).collect(),
                });
            }
        }
    }

    Ok(out)
}

fn nav_location_from_symbol(
    root: &Path,
    path: &Path,
    symbol: &GoSymbol,
    score: f64,
) -> Result<Option<NavLocation>, String> {
    let preview = read_line_preview(path, symbol.line)?;
    let relative_path = pathdiff::diff_paths(path, root)
        .unwrap_or_else(|| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Some(NavLocation {
        path: normalize_path(path).to_string_lossy().to_string(),
        relative_path,
        line: symbol.line,
        column: symbol.column,
        end_line: symbol.end_line,
        end_column: symbol.end_column,
        preview,
        score,
    }))
}

fn read_line_preview(path: &Path, line: usize) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    Ok(content
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim_end_matches('\r')
        .chars()
        .take(400)
        .collect())
}

fn push_unique_location(
    out: &mut Vec<NavLocation>,
    seen: &mut HashSet<String>,
    location: NavLocation,
) {
    let key = build_nav_key(&location);
    if seen.insert(key) {
        out.push(location);
    }
}

fn build_nav_key(location: &NavLocation) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        location.path, location.line, location.column, location.end_line, location.end_column
    )
}

fn score_go_definition_candidate(
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

fn resolve_go_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<GoFileAnalysis>>,
    entry: &GoSearchMatch,
    token: &str,
) -> Option<&'static str> {
    resolve_go_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_go_declaration(&entry.text, token))
}

fn is_go_declaration_location(
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
    match kind {
        "struct" => Some("struct"),
        "interface" => Some("interface"),
        "type" => Some("type"),
        "method" => Some("method"),
        "function" => Some("function"),
        "variable" => Some("variable"),
        "constant" => Some("constant"),
        _ => None,
    }
}

fn classify_go_declaration(line: &str, token: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if let Some((name, kind)) = extract_go_type_declaration(trimmed) {
        if name == token {
            return Some(match kind.as_str() {
                "struct" => "struct",
                "interface" => "interface",
                _ => "type",
            });
        }
    }
    if extract_go_method_name(trimmed).as_deref() == Some(token) {
        return Some("method");
    }
    if extract_go_function_name(trimmed).as_deref() == Some(token) {
        return Some("function");
    }
    if let Some((name, kind)) = extract_go_top_level_binding(trimmed) {
        if name == token {
            return Some(if kind == "constant" {
                "constant"
            } else {
                "variable"
            });
        }
    }
    if extract_go_short_var_name(trimmed).as_deref() == Some(token) {
        return Some("variable");
    }
    None
}

fn parse_go_single_import(line: &str) -> Option<GoImport> {
    let capture = IMPORT_SINGLE_RE.captures(line)?;
    Some(GoImport {
        path: capture.get(2)?.as_str().to_string(),
    })
}

fn parse_go_import_block_item(line: &str) -> Option<GoImport> {
    let capture = IMPORT_BLOCK_ITEM_RE.captures(line)?;
    Some(GoImport {
        path: capture.get(2)?.as_str().to_string(),
    })
}

fn extract_go_type_declaration(line: &str) -> Option<(String, String)> {
    if let Some(capture) = TYPE_RE.captures(line) {
        let name = capture.get(1)?.as_str().to_string();
        let kind = match capture.get(2).map(|item| item.as_str()) {
            Some("struct") => "struct",
            Some("interface") => "interface",
            _ => "type",
        };
        return Some((name, kind.to_string()));
    }
    let capture = TYPE_ALIAS_RE.captures(line)?;
    let name = capture.get(1)?.as_str().to_string();
    Some((name, "type".to_string()))
}

fn extract_go_method_name(line: &str) -> Option<String> {
    METHOD_RE
        .captures(line)
        .and_then(|capture| capture.get(1).map(|item| item.as_str().to_string()))
}

fn extract_go_function_name(line: &str) -> Option<String> {
    FUNCTION_RE
        .captures(line)
        .and_then(|capture| capture.get(1).map(|item| item.as_str().to_string()))
}

fn extract_go_top_level_binding(line: &str) -> Option<(String, String)> {
    if matches_go_non_binding_statement(line) {
        return None;
    }
    let capture = VAR_CONST_RE.captures(line)?;
    let kind = if capture.get(1).map(|item| item.as_str()) == Some("const") {
        "constant"
    } else {
        "variable"
    };
    Some((capture.get(2)?.as_str().to_string(), kind.to_string()))
}

fn extract_go_short_var_name(line: &str) -> Option<String> {
    if matches_go_non_binding_statement(line) {
        return None;
    }
    SHORT_VAR_RE
        .captures(line)
        .and_then(|capture| capture.get(1).map(|item| item.as_str().to_string()))
}

fn matches_go_non_binding_statement(line: &str) -> bool {
    [
        "return ", "go ", "defer ", "if ", "for ", "switch ", "case ", "select ", "package ",
        "import ", "func ", "type ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn strip_go_comments(line: &str, in_block_comment: &mut bool) -> String {
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
            if current == '\\' && next.is_some() && string_delim != '`' {
                out.push(next.unwrap());
                index += 2;
                continue;
            }
            if current == string_delim {
                in_string = false;
                string_delim = '\0';
            }
            index += 1;
            continue;
        }

        if matches!(current, '"' | '\'' | '`') {
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

fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
}

fn is_type_symbol(kind: &str) -> bool {
    matches!(kind, "struct" | "interface" | "type")
}

fn find_column(line: &str, token: &str) -> Option<usize> {
    line.find(token)
        .map(|offset| line[..offset].chars().count() + 1)
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

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{analyze_go_file, go_definition, go_references};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_go_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_go_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("helper")).expect("create helper dir");
        fs::write(root.join("go.mod"), "module demo\n\ngo 1.22\n").expect("write go.mod");
        root
    }

    #[test]
    fn go_document_symbols_detect_types_and_functions() {
        let root = make_temp_go_project();
        let path = root.join("main.go");
        fs::write(
            &path,
            r#"package main

type User struct{}

func (u User) Greet() {}

func Helper() {}
"#,
        )
        .expect("write main.go");

        let analysis = analyze_go_file(&path).expect("analyze go file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("User"), String::from("struct"))));
        assert!(names.contains(&(String::from("Greet"), String::from("method"))));
        assert!(names.contains(&(String::from("Helper"), String::from("function"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn go_definition_prefers_imported_package_function() {
        let root = make_temp_go_project();
        let helper = root.join("helper/helper.go");
        let main = root.join("main.go");
        fs::write(
            &helper,
            r#"package helper

func BuildUserRecord() string {
    return "ok"
}
"#,
        )
        .expect("write helper.go");
        fs::write(
            &main,
            r#"package main

import "demo/helper"

func main() {
    _ = helper.BuildUserRecord()
}
"#,
        )
        .expect("write main.go");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: main.clone(),
            relative_path: "main.go".to_string(),
            language: "go".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: main.to_string_lossy().to_string(),
            line: 6,
            column: 16,
        };

        let locations = go_definition(&ctx, &request).expect("resolve go definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("helper/helper.go") && item.line == 3),
            "expected helper package function definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn go_references_skip_definition_when_usage_exists() {
        let root = make_temp_go_project();
        let path = root.join("main.go");
        fs::write(
            &path,
            r#"package main

func greet() {
    name := "demo"
    println(name)
}
"#,
        )
        .expect("write main.go");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "main.go".to_string(),
            language: "go".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 5,
            column: 14,
        };

        let locations = go_references(&ctx, &request).expect("resolve go references");
        assert!(
            locations.iter().any(|item| item.line == 5),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 4),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}

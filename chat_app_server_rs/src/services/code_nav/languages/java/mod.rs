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

const JAVA_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".gradle",
];

const JAVA_EXTENSIONS: &[&str] = &["java"];
const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

static PACKAGE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*package\s+([A-Za-z_][A-Za-z0-9_.]*)\s*;").unwrap());
static IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*import\s+(static\s+)?([A-Za-z_][A-Za-z0-9_.]*(?:\.\*)?)\s*;").unwrap()
});
static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(class|interface|enum|record)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap()
});
static FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:@\w+(?:\([^)]*\))?\s*)*(?:(?:public|protected|private|static|final|transient|volatile)\s+)*(?:[\w.$\[\]<>?,]+\s+)+([A-Za-z_][A-Za-z0-9_]*)\s*(?:=[^;]*)?;\s*$",
    )
    .unwrap()
});

#[derive(Debug, Clone)]
struct JavaImport {
    path: String,
    is_static: bool,
    is_wildcard: bool,
}

#[derive(Debug, Clone)]
struct JavaSymbol {
    name: String,
    kind: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Clone)]
struct JavaFileAnalysis {
    package_name: Option<String>,
    imports: Vec<JavaImport>,
    symbols: Vec<JavaSymbol>,
    primary_type: Option<String>,
}

#[derive(Debug, Clone)]
struct JavaSearchMatch {
    path: String,
    relative_path: String,
    line: usize,
    column: usize,
    text: String,
}

fn indexed_java_symbols(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_java_file(path)?;
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

#[derive(Debug, Clone)]
struct TypeScope {
    name: String,
    body_depth: i32,
}

#[derive(Default)]
pub struct JavaCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for JavaCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "java"
    }

    fn language_id(&self) -> &'static str {
        "java"
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
        file_path.extension().and_then(|value| value.to_str()) == Some("java")
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("pom.xml").exists()
            || ctx.root.join("build.gradle").exists()
            || ctx.root.join("settings.gradle").exists()
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
        java_definition(ctx, req)
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        java_references(ctx, req)
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = analyze_java_file(&ctx.file_path)?;
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

fn java_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_java_file(&ctx.file_path)?;
    let resolved_type_paths = resolve_imported_type_paths(&ctx.root, &current, &token)?;
    let resolved_path_set: HashSet<String> = resolved_type_paths
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

    for path in resolved_type_paths {
        let analysis = analyze_java_file(&path)?;
        for symbol in analysis.symbols.iter().filter(|item| item.name == token) {
            let score = if is_type_like(&token) && is_type_symbol(&symbol.kind) {
                16.0
            } else {
                11.0
            };
            if let Some(location) = nav_location_from_symbol(&ctx.root, &path, symbol, score)? {
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "java",
        JAVA_EXTENSIONS,
        JAVA_IGNORED_DIRS,
        indexed_java_symbols,
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
            search_java_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
        if search_matches.is_empty() {
            search_matches =
                search_java_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
        }

        for entry in search_matches {
            let Some(declaration_kind) = resolve_java_declaration_kind(
                &mut analysis_cache,
                &entry,
                &token,
                current.primary_type.as_deref(),
            ) else {
                continue;
            };
            let score = score_java_definition_candidate(
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

fn java_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_java_file(&ctx.file_path)?;
    let mut matches =
        search_java_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
    if matches.is_empty() {
        matches = search_java_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
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
        if !seen.insert(key) {
            continue;
        }
        locations.push(location);
    }

    let mut declarations = Vec::new();
    let mut references = Vec::new();
    let mut classification_cache = HashMap::new();
    for location in locations {
        if is_java_declaration_location(
            &mut classification_cache,
            &location,
            &token,
            current.primary_type.as_deref(),
        ) {
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

fn analyze_java_file(path: &Path) -> Result<JavaFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
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

fn resolve_imported_type_paths(
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
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_java_path(entry))
    {
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

fn search_java_occurrences(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
) -> Result<Vec<JavaSearchMatch>, String> {
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
        .filter_entry(|entry| should_visit_java_path(entry))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("java") {
            continue;
        }
        let content = match fs::read_to_string(entry.path()) {
            Ok(content) => content,
            Err(_) => continue,
        };
        for (index, line) in content.lines().enumerate() {
            let normalized_line = line.trim_end_matches('\r');
            for found in regex.find_iter(normalized_line) {
                if out.len() >= max_results {
                    return Ok(out);
                }
                let column = normalized_line[..found.start()].chars().count() + 1;
                let relative_path = pathdiff::diff_paths(entry.path(), root)
                    .unwrap_or_else(|| entry.path().to_path_buf())
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(JavaSearchMatch {
                    path: normalize_path(entry.path()).to_string_lossy().to_string(),
                    relative_path,
                    line: index + 1,
                    column,
                    text: if normalized_line.len() > 400 {
                        normalized_line[..400].to_string()
                    } else {
                        normalized_line.to_string()
                    },
                });
            }
        }
    }

    Ok(out)
}

fn nav_location_from_symbol(
    root: &Path,
    path: &Path,
    symbol: &JavaSymbol,
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

fn score_java_definition_candidate(
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

fn resolve_java_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<JavaFileAnalysis>>,
    entry: &JavaSearchMatch,
    token: &str,
    current_type_name: Option<&str>,
) -> Option<&'static str> {
    resolve_java_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_java_declaration(&entry.text, token, current_type_name))
}

fn is_java_declaration_location(
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
    match kind {
        "class" => Some("class"),
        "interface" => Some("interface"),
        "enum" => Some("enum"),
        "record" => Some("record"),
        "constructor" => Some("constructor"),
        "method" => Some("method"),
        "field" => Some("field"),
        _ => None,
    }
}

fn classify_java_declaration(
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

fn extract_method_signature(line: &str, current_type_name: &str) -> Option<(String, String)> {
    let line = strip_leading_java_annotations(line.trim());
    if line.is_empty() || matches_java_non_method_statement(line) {
        return None;
    }

    let open_paren = line.find('(')?;
    let before_params = line[..open_paren].trim_end();
    if let Some(close_end) = matching_java_paren_end(&line[open_paren..]) {
        let after_params = line.get(open_paren + close_end..).unwrap_or("");
        if !is_java_method_suffix(after_params) {
            return None;
        }
    }
    if before_params.contains('=') || before_params.contains("->") {
        return None;
    }

    let name = last_identifier(before_params)?;
    if matches!(
        name.as_str(),
        "if" | "for" | "while" | "switch" | "catch" | "return" | "throw" | "new"
    ) {
        return None;
    }

    let kind = if !current_type_name.is_empty() && name == current_type_name {
        "constructor"
    } else {
        let declaration_prefix = before_params.strip_suffix(name.as_str())?.trim_end();
        if !has_java_method_return_type(declaration_prefix) {
            return None;
        }
        "method"
    };
    Some((name, kind.to_string()))
}

fn strip_leading_java_annotations(mut line: &str) -> &str {
    loop {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('@') {
            return trimmed;
        }
        let Some(rest) = consume_java_annotation(trimmed) else {
            return trimmed;
        };
        line = rest;
        if line.trim().is_empty() {
            return "";
        }
    }
}

fn consume_java_annotation(line: &str) -> Option<&str> {
    let mut index = '@'.len_utf8();
    let name = line.get(index..)?;
    let mut chars = name.char_indices();
    let (_, first) = chars.next()?;
    if !is_java_identifier_start(first) {
        return None;
    }
    index += first.len_utf8();

    for (offset, ch) in chars {
        if is_java_identifier_part(ch) || ch == '.' {
            index = '@'.len_utf8() + offset + ch.len_utf8();
        } else {
            break;
        }
    }

    let rest = line.get(index..)?.trim_start();
    if rest.starts_with('(') {
        let end = matching_java_paren_end(rest)?;
        return rest.get(end..);
    }
    Some(rest)
}

fn matching_java_paren_end(value: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_delim = '\0';
    let mut escaped = false;

    for (index, ch) in value.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == string_delim {
                in_string = false;
                string_delim = '\0';
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_string = true;
            string_delim = ch;
            continue;
        }

        if ch == '(' {
            depth += 1;
            continue;
        }
        if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(index + ch.len_utf8());
            }
        }
    }

    None
}

fn matches_java_non_method_statement(line: &str) -> bool {
    [
        "return ", "throw ", "package ", "import ", "if ", "for ", "while ", "switch ", "case ",
        "new ", "assert ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn is_java_method_suffix(value: &str) -> bool {
    let suffix = value.trim_start();
    suffix.is_empty()
        || suffix.starts_with('{')
        || suffix.starts_with(';')
        || suffix.starts_with("throws ")
        || suffix.starts_with("default ")
}

fn has_java_method_return_type(prefix: &str) -> bool {
    let mut rest = prefix.trim();
    loop {
        let Some((word, after_word)) = split_first_java_word(rest) else {
            break;
        };
        if !is_java_modifier(word) {
            break;
        }
        rest = after_word.trim_start();
    }

    if rest.starts_with('<') {
        if let Some(end) = matching_java_angle_end(rest) {
            rest = rest.get(end..).unwrap_or("").trim_start();
        }
    }

    !rest.is_empty()
}

fn split_first_java_word(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start();
    let mut end = None;
    for (index, ch) in value.char_indices() {
        if index == 0 && !is_java_identifier_start(ch) {
            return None;
        }
        if is_java_identifier_part(ch) {
            end = Some(index + ch.len_utf8());
        } else {
            break;
        }
    }
    let end = end?;
    Some((&value[..end], &value[end..]))
}

fn is_java_modifier(value: &str) -> bool {
    matches!(
        value,
        "public"
            | "protected"
            | "private"
            | "static"
            | "final"
            | "abstract"
            | "synchronized"
            | "native"
            | "default"
            | "strictfp"
    )
}

fn matching_java_angle_end(value: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (index, ch) in value.char_indices() {
        if ch == '<' {
            depth += 1;
            continue;
        }
        if ch == '>' {
            depth -= 1;
            if depth == 0 {
                return Some(index + ch.len_utf8());
            }
        }
    }
    None
}

fn is_java_identifier_start(value: char) -> bool {
    value == '_' || value == '$' || value.is_alphabetic()
}

fn is_java_identifier_part(value: char) -> bool {
    value == '_' || value == '$' || value.is_alphanumeric()
}

fn extract_field_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if line.contains('(') {
        return None;
    }
    if matches_java_non_field_statement(trimmed) {
        return None;
    }
    if let Some(capture) = FIELD_RE.captures(line) {
        return Some(capture.get(1)?.as_str().to_string());
    }

    if !trimmed.ends_with(';') {
        return None;
    }

    let declaration_head = trimmed
        .trim_end_matches(';')
        .split('=')
        .next()
        .unwrap_or("")
        .trim_end();
    last_identifier(declaration_head)
}

fn matches_java_non_field_statement(line: &str) -> bool {
    [
        "return ", "throw ", "package ", "import ", "if ", "for ", "while ", "switch ", "case ",
        "new ", "assert ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
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

fn find_column(line: &str, token: &str) -> Option<usize> {
    line.find(token)
        .map(|offset| line[..offset].chars().count() + 1)
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

fn strip_java_comments(line: &str, in_block_comment: &mut bool) -> String {
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
            if current == '\\' && next.is_some() {
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

fn count_char(value: &str, needle: char) -> usize {
    value.chars().filter(|ch| *ch == needle).count()
}

fn last_identifier(value: &str) -> Option<String> {
    let mut end = None;
    for (index, ch) in value.char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            end = Some(index + ch.len_utf8());
            break;
        }
    }
    let end = end?;

    let mut start = end;
    for (index, ch) in value[..end].char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            start = index;
        } else {
            break;
        }
    }

    let candidate = value[start..end].trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
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
    use super::{
        analyze_java_file, classify_java_declaration, extract_field_name, extract_method_signature,
        java_definition, java_references,
    };
    use crate::services::code_nav::fallback::extract_token_at_position;
    use crate::services::code_nav::types::NavPositionRequest;
    use crate::services::code_nav::types::ProjectContext;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_java_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_java_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("src/main/java/com/example")).expect("create source dir");
        fs::write(root.join("pom.xml"), "<project/>").expect("write pom");
        root
    }

    #[test]
    fn java_document_symbols_detect_type_and_members() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/Sample.java");
        fs::write(
            &path,
            r#"package com.example;

public class Sample {
    private String name;

    public Sample() {}

    public String greet(String who) {
        return name + who;
    }
}
"#,
        )
        .expect("write java file");

        let analysis = analyze_java_file(&path).expect("analyze java file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("Sample"), String::from("class"))));
        assert!(names.contains(&(String::from("Sample"), String::from("constructor"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("name"), String::from("field"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_document_symbols_ignore_annotation_line_and_detect_bean_method() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/KafkaConfig.java");
        fs::write(
            &path,
            r#"package com.example;

public class KafkaConfig {
    @Bean("kafkaProducerFactory")
    public ProducerFactory<Object, Object> kafkaProducerFactory(
        ObjectProvider<DefaultKafkaProducerFactoryCustomizer> customizers
    ) {
        return null;
    }
}
"#,
        )
        .expect("write KafkaConfig");

        let analysis = analyze_java_file(&path).expect("analyze java file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("KafkaConfig"), String::from("class"))));
        assert!(names.contains(&(String::from("kafkaProducerFactory"), String::from("method"))));
        assert!(!names.contains(&(String::from("n"), String::from("method"))));
        assert!(!names.contains(&(String::from("Bean"), String::from("method"))));
        assert!(
            extract_method_signature("@Bean(\"kafkaProducerFactory\")", "KafkaConfig").is_none()
        );
        assert_eq!(
            extract_method_signature(
                "@Bean(\"kafkaProducerFactory\") public ProducerFactory<Object, Object> kafkaProducerFactory() {",
                "KafkaConfig"
            )
            .map(|(name, kind)| (name, kind)),
            Some((String::from("kafkaProducerFactory"), String::from("method")))
        );
        assert_eq!(
            extract_method_signature(
                "public void configured(@Qualifier(\"main\") String name,",
                "KafkaConfig"
            )
            .map(|(name, kind)| (name, kind)),
            Some((String::from("configured"), String::from("method")))
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_field_heuristics_detect_simple_field_declaration() {
        assert_eq!(
            extract_field_name("private String name;").as_deref(),
            Some("name")
        );
        assert_eq!(
            classify_java_declaration("private String name;", "name", Some("Sample")),
            Some("field")
        );
        assert_eq!(extract_field_name("return name;"), None);
        assert_eq!(
            classify_java_declaration("return name;", "name", Some("Sample")),
            None
        );
    }

    #[test]
    fn java_extract_token_reads_field_usage_identifier() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/TokenSample.java");
        fs::write(
            &path,
            r#"package com.example;

public class TokenSample {
    private String name;

    public String greet() {
        return name;
    }
}
"#,
        )
        .expect("write TokenSample");

        let token = extract_token_at_position(&path, 7, 16).expect("extract token");
        assert_eq!(token.as_deref(), Some("name"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_definition_prefers_imported_type_file() {
        let root = make_temp_java_project();
        let foo = root.join("src/main/java/com/example/Foo.java");
        let bar = root.join("src/main/java/com/example/Bar.java");
        fs::write(
            &foo,
            r#"package com.example;

public class Foo {
}
"#,
        )
        .expect("write Foo");
        fs::write(
            &bar,
            r#"package com.example;

import com.example.Foo;

public class Bar {
    private Foo foo = new Foo();
}
"#,
        )
        .expect("write Bar");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: bar.clone(),
            relative_path: "src/main/java/com/example/Bar.java".to_string(),
            language: "java".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: bar.to_string_lossy().to_string(),
            line: 6,
            column: 13,
        };

        let locations = java_definition(&ctx, &request).expect("resolve java definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("com/example/Foo.java") && item.line == 3),
            "expected Foo.java class definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_references_skip_definition_when_usage_exists() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/RefSample.java");
        fs::write(
            &path,
            r#"package com.example;

public class RefSample {
    private String name;

    public String greet() {
        return name;
    }
}
"#,
        )
        .expect("write RefSample");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "src/main/java/com/example/RefSample.java".to_string(),
            language: "java".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 7,
            column: 16,
        };

        let locations = java_references(&ctx, &request).expect("resolve java references");
        assert!(
            locations.iter().any(|item| item.line == 7),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 4),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}

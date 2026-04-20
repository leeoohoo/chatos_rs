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

const RUST_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
];

const RUST_EXTENSIONS: &[&str] = &["rs"];
const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

static TYPE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(struct|enum|trait|type|mod)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static FN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    )
    .unwrap()
});
static CONST_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+([A-Za-z_][A-Za-z0-9_]*)\b")
        .unwrap()
});
static LET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*let(?:\s+mut)?\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());
static IMPL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*impl\b.*").unwrap());

#[derive(Debug, Clone)]
struct RustSymbol {
    name: String,
    kind: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Clone)]
struct RustFileAnalysis {
    symbols: Vec<RustSymbol>,
}

#[derive(Debug, Clone)]
struct RustSearchMatch {
    path: String,
    relative_path: String,
    line: usize,
    column: usize,
    text: String,
}

fn indexed_rust_symbols(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_rust_file(path)?;
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
struct BlockScope {
    kind: String,
    body_depth: i32,
}

#[derive(Default)]
pub struct RustCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for RustCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "rust"
    }

    fn language_id(&self) -> &'static str {
        "rust"
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
        file_path.extension().and_then(|value| value.to_str()) == Some("rs")
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("Cargo.toml").exists()
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
        rust_definition(ctx, req)
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        rust_references(ctx, req)
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = analyze_rust_file(&ctx.file_path)?;
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

fn rust_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_rust_file(&ctx.file_path)?;
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

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "rust",
        RUST_EXTENSIONS,
        RUST_IGNORED_DIRS,
        indexed_rust_symbols,
    ) {
        if let Some(symbols) = index.symbols_by_name.get(&token) {
            for indexed in symbols {
                if indexed.relative_path == ctx.relative_path && indexed.symbol.line == req.line {
                    continue;
                }
                let score = score_indexed_definition_candidate(ctx, req, indexed);
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
            search_rust_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
        if search_matches.is_empty() {
            search_matches =
                search_rust_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
        }

        for entry in search_matches {
            let Some(declaration_kind) =
                resolve_rust_declaration_kind(&mut analysis_cache, &entry, &token)
            else {
                continue;
            };
            let score = score_rust_definition_candidate(ctx, req, &token, declaration_kind, &entry);
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

fn rust_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let mut matches =
        search_rust_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
    if matches.is_empty() {
        matches = search_rust_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
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
        if is_rust_declaration_location(&mut classification_cache, &location, &token) {
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

fn analyze_rust_file(path: &Path) -> Result<RustFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
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

fn search_rust_occurrences(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
) -> Result<Vec<RustSearchMatch>, String> {
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
        .filter_entry(|entry| should_visit_rust_path(entry))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("rs") {
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
                out.push(RustSearchMatch {
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
    symbol: &RustSymbol,
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

fn score_rust_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    declaration_kind: &str,
    entry: &RustSearchMatch,
) -> f64 {
    let mut score = 0.0;
    let is_same_file = entry.relative_path == ctx.relative_path;
    let is_same_line = is_same_file && entry.line == req.line;
    let file_stem = Path::new(&entry.relative_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");

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
        "struct" | "enum" | "trait" | "type" | "module" => 7.0,
        "method" | "function" => 5.0,
        "constant" | "variable" => 3.0,
        _ => 1.0,
    };

    if is_type_like(token) && matches!(declaration_kind, "struct" | "enum" | "trait" | "type") {
        score += 2.0;
    }

    score
}

fn resolve_rust_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<RustFileAnalysis>>,
    entry: &RustSearchMatch,
    token: &str,
) -> Option<&'static str> {
    resolve_rust_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_rust_declaration(&entry.text, token))
}

fn is_rust_declaration_location(
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
    match kind {
        "struct" => Some("struct"),
        "enum" => Some("enum"),
        "trait" => Some("trait"),
        "type" => Some("type"),
        "module" => Some("module"),
        "method" => Some("method"),
        "function" => Some("function"),
        "constant" => Some("constant"),
        "variable" => Some("variable"),
        _ => None,
    }
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

fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
}

fn find_column(line: &str, token: &str) -> Option<usize> {
    line.find(token)
        .map(|offset| line[..offset].chars().count() + 1)
}

fn should_visit_rust_path(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !RUST_IGNORED_DIRS.contains(&name)
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
    use super::{analyze_rust_file, rust_definition, rust_references};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_rust_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_rust_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = 'demo'\nversion = '0.1.0'\n",
        )
        .expect("write cargo");
        root
    }

    #[test]
    fn rust_document_symbols_detect_types_and_functions() {
        let root = make_temp_rust_project();
        let path = root.join("src/lib.rs");
        fs::write(
            &path,
            r#"pub struct User;

impl User {
    pub fn greet(&self) {}
}

pub fn helper() {}
"#,
        )
        .expect("write rust file");

        let analysis = analyze_rust_file(&path).expect("analyze rust file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("User"), String::from("struct"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("helper"), String::from("function"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rust_definition_prefers_function_declaration() {
        let root = make_temp_rust_project();
        let foo = root.join("src/foo.rs");
        let main = root.join("src/main.rs");
        fs::write(
            &foo,
            r#"pub fn build_user_record() -> &'static str {
    "ok"
}
"#,
        )
        .expect("write foo");
        fs::write(
            &main,
            r#"mod foo;

fn main() {
    let _ = foo::build_user_record();
}
"#,
        )
        .expect("write main");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: main.clone(),
            relative_path: "src/main.rs".to_string(),
            language: "rust".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: main.to_string_lossy().to_string(),
            line: 4,
            column: 19,
        };

        let locations = rust_definition(&ctx, &request).expect("resolve rust definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("src/foo.rs") && item.line == 1),
            "expected foo.rs function definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rust_references_skip_definition_when_usage_exists() {
        let root = make_temp_rust_project();
        let path = root.join("src/lib.rs");
        fs::write(
            &path,
            r#"pub fn greet() {
    let name = "demo";
    println!("{}", name);
}
"#,
        )
        .expect("write lib");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 3,
            column: 20,
        };

        let locations = rust_references(&ctx, &request).expect("resolve rust references");
        assert!(
            locations.iter().any(|item| item.line == 3),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 2),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}

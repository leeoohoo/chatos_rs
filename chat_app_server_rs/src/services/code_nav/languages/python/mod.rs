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

const PYTHON_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".venv",
    "venv",
    "__pycache__",
];

const PYTHON_EXTENSIONS: &[&str] = &["py"];
const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

static CLASS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());
static DEF_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());
static FROM_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*from\s+([A-Za-z_][A-Za-z0-9_.]*)\s+import\s+(.+)$").unwrap());
static IMPORT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*import\s+(.+)$").unwrap());
static ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:self\.)?([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=]+)?=\s*.+$").unwrap()
});

#[derive(Debug, Clone)]
struct PythonImport {
    module: String,
    symbol_name: String,
    alias: Option<String>,
}

#[derive(Debug, Clone)]
struct PythonSymbol {
    name: String,
    kind: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Clone)]
struct PythonFileAnalysis {
    imports: Vec<PythonImport>,
    symbols: Vec<PythonSymbol>,
}

#[derive(Debug, Clone)]
struct PythonSearchMatch {
    path: String,
    relative_path: String,
    line: usize,
    column: usize,
    text: String,
}

fn indexed_python_symbols(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_python_file(path)?;
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
struct PythonClassScope {
    indent: usize,
}

#[derive(Debug, Clone)]
struct ResolvedPythonImport {
    symbol_name: String,
    path: PathBuf,
}

#[derive(Default)]
pub struct PythonCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for PythonCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "python"
    }

    fn language_id(&self) -> &'static str {
        "python"
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
        file_path.extension().and_then(|value| value.to_str()) == Some("py")
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("pyproject.toml").exists()
            || ctx.root.join("requirements.txt").exists()
            || ctx.root.join("setup.py").exists()
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
        python_definition(ctx, req)
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        python_references(ctx, req)
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = analyze_python_file(&ctx.file_path)?;
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

fn python_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_python_file(&ctx.file_path)?;
    let resolved_imports = resolve_imported_symbol_paths(&ctx.root, &current, &token)?;
    let resolved_path_set: HashSet<String> = resolved_imports
        .iter()
        .map(|item| item.path.to_string_lossy().to_string())
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

    for resolved in resolved_imports {
        let analysis = analyze_python_file(&resolved.path)?;
        for symbol in analysis
            .symbols
            .iter()
            .filter(|item| item.name == resolved.symbol_name)
        {
            let score = if is_type_like(&resolved.symbol_name) {
                15.0
            } else {
                12.0
            };
            if let Some(location) =
                nav_location_from_symbol(&ctx.root, &resolved.path, symbol, score)?
            {
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "python",
        PYTHON_EXTENSIONS,
        PYTHON_IGNORED_DIRS,
        indexed_python_symbols,
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
            search_python_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
        if search_matches.is_empty() {
            search_matches =
                search_python_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
        }

        for entry in search_matches {
            let Some(declaration_kind) =
                resolve_python_declaration_kind(&mut analysis_cache, &entry, &token)
            else {
                continue;
            };
            let score = score_python_definition_candidate(
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

fn python_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let mut matches =
        search_python_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
    if matches.is_empty() {
        matches = search_python_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
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
        if is_python_declaration_location(&mut classification_cache, &location, &token) {
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

fn analyze_python_file(path: &Path) -> Result<PythonFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut imports = Vec::new();
    let mut symbols = Vec::new();
    let mut class_stack: Vec<PythonClassScope> = Vec::new();

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let sanitized = strip_python_comment(raw_line);
        let trimmed = sanitized.trim();
        if trimmed.is_empty() {
            continue;
        }

        let indent = indent_width(raw_line);
        while class_stack
            .last()
            .map(|scope| indent <= scope.indent)
            .unwrap_or(false)
        {
            class_stack.pop();
        }

        if let Some(capture) = FROM_IMPORT_RE.captures(trimmed) {
            let module = capture[1].to_string();
            for part in capture[2].split(',') {
                let chunk = part.trim();
                if chunk.is_empty() || chunk == "*" {
                    continue;
                }
                let (symbol_name, alias) = parse_import_alias(chunk);
                imports.push(PythonImport {
                    module: module.clone(),
                    symbol_name,
                    alias,
                });
            }
            continue;
        }

        if let Some(capture) = IMPORT_RE.captures(trimmed) {
            for part in capture[1].split(',') {
                let chunk = part.trim();
                if chunk.is_empty() {
                    continue;
                }
                let (module, alias) = parse_import_alias(chunk);
                let module_name = alias.clone().unwrap_or_else(|| {
                    module
                        .rsplit('.')
                        .next()
                        .unwrap_or(module.as_str())
                        .to_string()
                });
                imports.push(PythonImport {
                    module,
                    symbol_name: module_name,
                    alias,
                });
            }
            continue;
        }

        if let Some(capture) = CLASS_RE.captures(trimmed) {
            let name = capture[1].to_string();
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            symbols.push(PythonSymbol {
                name,
                kind: "class".to_string(),
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
            class_stack.push(PythonClassScope { indent });
            continue;
        }

        if let Some(capture) = DEF_RE.captures(trimmed) {
            let name = capture[1].to_string();
            let column = find_column(raw_line, &name).unwrap_or(1);
            let end_column = column + name.chars().count().saturating_sub(1);
            let kind = if class_stack.is_empty() {
                "function"
            } else {
                "method"
            };
            symbols.push(PythonSymbol {
                name,
                kind: kind.to_string(),
                line: line_number,
                column,
                end_line: line_number,
                end_column,
            });
            continue;
        }

        if indent == 0 {
            if let Some(name) = extract_assigned_name(trimmed) {
                let column = find_column(raw_line, &name).unwrap_or(1);
                let end_column = column + name.chars().count().saturating_sub(1);
                symbols.push(PythonSymbol {
                    name,
                    kind: "variable".to_string(),
                    line: line_number,
                    column,
                    end_line: line_number,
                    end_column,
                });
            }
        }
    }

    symbols.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.name.cmp(&right.name))
    });

    Ok(PythonFileAnalysis { imports, symbols })
}

fn resolve_imported_symbol_paths(
    root: &Path,
    analysis: &PythonFileAnalysis,
    token: &str,
) -> Result<Vec<ResolvedPythonImport>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for item in &analysis.imports {
        let imported_name = item.alias.as_deref().unwrap_or(item.symbol_name.as_str());
        if imported_name != token && item.symbol_name != token {
            continue;
        }

        for path in resolve_python_module_paths(root, &item.module)? {
            let key = format!("{}:{}", path.to_string_lossy(), item.symbol_name);
            if seen.insert(key) {
                out.push(ResolvedPythonImport {
                    symbol_name: item.symbol_name.clone(),
                    path,
                });
            }
        }
    }

    Ok(out)
}

fn resolve_python_module_paths(root: &Path, module: &str) -> Result<Vec<PathBuf>, String> {
    let relative = PathBuf::from(module.replace('.', "/"));
    let candidates = [
        root.join(&relative).with_extension("py"),
        root.join(&relative).join("__init__.py"),
        root.join("src").join(&relative).with_extension("py"),
        root.join("src").join(&relative).join("__init__.py"),
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

    if !out.is_empty() {
        return Ok(out);
    }

    let target_name = format!("{}.py", module.rsplit('.').next().unwrap_or(module));
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_python_path(entry))
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
        let normalized_str = normalized.to_string_lossy().replace('\\', "/");
        let dotted = normalized_str
            .trim_start_matches(root.to_string_lossy().as_ref())
            .trim_start_matches('/')
            .trim_start_matches("src/")
            .trim_end_matches(".py")
            .trim_end_matches("/__init__")
            .replace('/', ".");
        if dotted.ends_with(module) {
            out.push(normalized);
        }
    }

    Ok(out)
}

fn search_python_occurrences(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
) -> Result<Vec<PythonSearchMatch>, String> {
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
        .filter_entry(|entry| should_visit_python_path(entry))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("py") {
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
                out.push(PythonSearchMatch {
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
    symbol: &PythonSymbol,
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

fn score_python_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    declaration_kind: &str,
    entry: &PythonSearchMatch,
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
        "class" => 7.0,
        "method" | "function" => 5.0,
        "variable" => 3.0,
        _ => 1.0,
    };

    if is_type_like(token) && declaration_kind == "class" {
        score += 2.0;
    }

    score
}

fn resolve_python_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<PythonFileAnalysis>>,
    entry: &PythonSearchMatch,
    token: &str,
) -> Option<&'static str> {
    resolve_python_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_python_declaration(&entry.text, token))
}

fn is_python_declaration_location(
    analysis_cache: &mut HashMap<String, Option<PythonFileAnalysis>>,
    location: &NavLocation,
    token: &str,
) -> bool {
    let entry = PythonSearchMatch {
        path: location.path.clone(),
        relative_path: location.relative_path.clone(),
        line: location.line,
        column: location.column,
        text: location.preview.clone(),
    };
    resolve_python_declaration_kind(analysis_cache, &entry, token).is_some()
}

fn resolve_python_symbol_kind(
    analysis_cache: &mut HashMap<String, Option<PythonFileAnalysis>>,
    entry: &PythonSearchMatch,
    token: &str,
) -> Option<&'static str> {
    let analysis = cached_python_analysis(analysis_cache, &entry.path)?;
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

fn cached_python_analysis<'a>(
    analysis_cache: &'a mut HashMap<String, Option<PythonFileAnalysis>>,
    path: &str,
) -> Option<&'a PythonFileAnalysis> {
    if !analysis_cache.contains_key(path) {
        analysis_cache.insert(path.to_string(), analyze_python_file(Path::new(path)).ok());
    }
    analysis_cache.get(path).and_then(|item| item.as_ref())
}

fn declaration_kind_from_symbol_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "class" => Some("class"),
        "method" => Some("method"),
        "function" => Some("function"),
        "variable" => Some("variable"),
        _ => None,
    }
}

fn classify_python_declaration(line: &str, token: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if let Some(capture) = CLASS_RE.captures(trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("class");
        }
    }
    if let Some(capture) = DEF_RE.captures(trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("function");
        }
    }
    if extract_assigned_name(trimmed).as_deref() == Some(token) {
        return Some("variable");
    }
    None
}

fn extract_assigned_name(line: &str) -> Option<String> {
    if matches_python_non_assignment(line) {
        return None;
    }
    ASSIGN_RE
        .captures(line)
        .and_then(|capture| capture.get(1).map(|item| item.as_str().to_string()))
}

fn matches_python_non_assignment(line: &str) -> bool {
    [
        "return ",
        "raise ",
        "import ",
        "from ",
        "if ",
        "elif ",
        "for ",
        "while ",
        "assert ",
        "with ",
        "except ",
        "yield ",
        "class ",
        "def ",
        "async def ",
        "@",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn parse_import_alias(value: &str) -> (String, Option<String>) {
    let mut parts = value.splitn(2, " as ");
    let name = parts.next().unwrap_or("").trim().to_string();
    let alias = parts.next().map(|item| item.trim().to_string());
    (name, alias.filter(|item| !item.is_empty()))
}

fn strip_python_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            out.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' && (in_single || in_double) {
            out.push(ch);
            escape = true;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            out.push(ch);
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            out.push(ch);
            continue;
        }
        if ch == '#' && !in_single && !in_double {
            break;
        }
        out.push(ch);
    }

    out
}

fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .count()
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

fn should_visit_python_path(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !PYTHON_IGNORED_DIRS.contains(&name)
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
    use super::{analyze_python_file, python_definition, python_references};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_python_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_python_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("app")).expect("create package dir");
        fs::write(root.join("pyproject.toml"), "[project]\nname = 'demo'\n")
            .expect("write pyproject");
        root
    }

    #[test]
    fn python_document_symbols_detect_classes_and_functions() {
        let root = make_temp_python_project();
        let path = root.join("app/sample.py");
        fs::write(
            &path,
            r#"class Sample:
    def greet(self, who):
        return who

def helper():
    return "ok"
"#,
        )
        .expect("write sample python file");

        let analysis = analyze_python_file(&path).expect("analyze python file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("Sample"), String::from("class"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("helper"), String::from("function"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn python_definition_prefers_imported_function_file() {
        let root = make_temp_python_project();
        let helpers = root.join("app/helpers.py");
        let main = root.join("app/main.py");
        fs::write(
            &helpers,
            r#"def greet():
    return "hello"
"#,
        )
        .expect("write helpers");
        fs::write(
            &main,
            r#"from app.helpers import greet

def run():
    return greet()
"#,
        )
        .expect("write main");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: main.clone(),
            relative_path: "app/main.py".to_string(),
            language: "python".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: main.to_string_lossy().to_string(),
            line: 4,
            column: 14,
        };

        let locations = python_definition(&ctx, &request).expect("resolve python definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("app/helpers.py") && item.line == 1),
            "expected helpers.py function definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn python_references_skip_definition_when_usage_exists() {
        let root = make_temp_python_project();
        let path = root.join("app/sample.py");
        fs::write(
            &path,
            r#"name = "demo"

def greet():
    return name
"#,
        )
        .expect("write sample");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "app/sample.py".to_string(),
            language: "python".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 4,
            column: 13,
        };

        let locations = python_references(&ctx, &request).expect("resolve python references");
        assert!(
            locations.iter().any(|item| item.line == 4),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 1),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}

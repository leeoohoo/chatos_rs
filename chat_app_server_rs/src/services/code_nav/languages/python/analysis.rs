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

pub(crate) const PYTHON_IGNORED_DIRS: &[&str] = &[
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

pub(crate) const PYTHON_EXTENSIONS: &[&str] = &["py"];

static CLASS_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)\b"));
static DEF_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\b"));
static FROM_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*from\s+([A-Za-z_][A-Za-z0-9_.]*)\s+import\s+(.+)$"));
static IMPORT_RE: Lazy<Regex> = Lazy::new(|| compile_static_regex(r"^\s*import\s+(.+)$"));
static ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(r"^\s*(?:self\.)?([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=]+)?=\s*.+$")
});

#[derive(Debug, Clone)]
pub(crate) struct PythonImport {
    pub(crate) module: String,
    pub(crate) symbol_name: String,
    pub(crate) alias: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PythonSymbol {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PythonFileAnalysis {
    pub(crate) imports: Vec<PythonImport>,
    pub(crate) symbols: Vec<PythonSymbol>,
}

#[derive(Debug, Clone)]
pub(crate) struct PythonSearchMatch {
    pub(crate) path: String,
    pub(crate) relative_path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) text: String,
}

#[derive(Debug, Clone)]
struct PythonClassScope {
    indent: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedPythonImport {
    pub(crate) symbol_name: String,
    pub(crate) path: PathBuf,
}

pub(crate) fn analyze_python_file(path: &Path) -> Result<PythonFileAnalysis, String> {
    let content = read_code_nav_file_to_string(path)?;
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

pub(crate) fn resolve_imported_symbol_paths(
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

pub(crate) fn search_python_occurrences(
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
    let started_at = Instant::now();
    let mut visited_entries = 0usize;

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_python_path(entry))
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
        if entry.path().extension().and_then(|value| value.to_str()) != Some("py") {
            continue;
        }
        let content = match read_code_nav_file_to_string(entry.path()) {
            Ok(content) => content,
            Err(_) => continue,
        };
        for (index, line) in content.lines().enumerate() {
            if index % 128 == 0 {
                ensure_code_nav_text_search_budget(started_at, visited_entries)?;
            }

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
                    text: truncate_preview(normalized_line, 400),
                });
            }
        }
    }

    Ok(out)
}

pub(crate) fn nav_location_from_symbol(
    root: &Path,
    path: &Path,
    symbol: &PythonSymbol,
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

pub(crate) fn score_python_definition_candidate(
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

pub(crate) fn resolve_python_declaration_kind(
    analysis_cache: &mut HashMap<String, Option<PythonFileAnalysis>>,
    entry: &PythonSearchMatch,
    token: &str,
) -> Option<&'static str> {
    resolve_python_symbol_kind(analysis_cache, entry, token)
        .or_else(|| classify_python_declaration(&entry.text, token))
}

pub(crate) fn is_python_declaration_location(
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
    let started_at = Instant::now();
    let mut visited_entries = 0usize;
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_python_path(entry))
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
    shared_declaration_kind_from_symbol_kind(kind)
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

fn should_visit_python_path(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !PYTHON_IGNORED_DIRS.contains(&name)
}

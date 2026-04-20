use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::RegexBuilder;
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::symbol_index::{
    nav_location_from_indexed_symbol, project_symbol_index, score_indexed_definition_candidate,
    IndexedSymbol,
};
use crate::services::code_nav::types::{
    DocumentSymbolItem, DocumentSymbolsResponse, NavCapabilities, NavLocation, NavPositionRequest,
    ProjectContext,
};

const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

#[derive(Debug, Clone)]
pub struct BasicSymbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone)]
pub struct BasicFileAnalysis {
    pub symbols: Vec<BasicSymbol>,
}

#[derive(Debug, Clone)]
struct BasicSearchMatch {
    path: String,
    relative_path: String,
    line: usize,
    column: usize,
    text: String,
}

pub struct BasicLanguageSpec {
    pub provider_id: &'static str,
    pub language_id: &'static str,
    pub extensions: &'static [&'static str],
    pub ignored_dirs: &'static [&'static str],
    pub project_files: &'static [&'static str],
    pub project_extensions: &'static [&'static str],
    pub analyze_file: fn(&Path) -> Result<BasicFileAnalysis, String>,
    pub classify_declaration: fn(&str, &str) -> Option<&'static str>,
}

impl BasicLanguageSpec {
    pub fn supports_file(&self, file_path: &Path) -> bool {
        extension_matches(file_path, self.extensions)
    }

    pub fn detect_project(&self, ctx: &ProjectContext) -> bool {
        self.project_files
            .iter()
            .any(|marker| ctx.root.join(marker).exists())
            || fs::read_dir(&ctx.root)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.filter_map(Result::ok))
                .any(|entry| extension_matches(&entry.path(), self.project_extensions))
    }

    pub fn capabilities(&self) -> NavCapabilities {
        NavCapabilities {
            supports_definition: true,
            supports_references: true,
            supports_document_symbols: true,
        }
    }

    pub fn document_symbols(
        &self,
        ctx: &ProjectContext,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = (self.analyze_file)(&ctx.file_path)?;
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
            provider: self.provider_id.to_string(),
            language: self.language_id.to_string(),
            mode: "provider-heuristic".to_string(),
            symbols,
        })
    }

    pub fn definition(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
        let Some(token) = token else {
            return Ok(Vec::new());
        };

        let current = (self.analyze_file)(&ctx.file_path)?;
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();

        for symbol in current
            .symbols
            .iter()
            .filter(|item| item.name == token && item.line != req.line)
        {
            if let Some(location) =
                nav_location_from_symbol(&ctx.root, &ctx.file_path, symbol, 9.0)?
            {
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }

        if let Ok(index) = project_symbol_index(
            ctx.root.as_path(),
            self.provider_id,
            self.extensions,
            self.ignored_dirs,
            |path| indexed_basic_symbols(path, self.analyze_file),
        ) {
            if let Some(symbols) = index.symbols_by_name.get(&token) {
                for indexed in symbols {
                    if indexed.relative_path == ctx.relative_path && indexed.symbol.line == req.line
                    {
                        continue;
                    }
                    let score = score_indexed_definition_candidate(ctx, req, indexed);
                    let location = match nav_location_from_indexed_symbol(&ctx.root, indexed, score)
                    {
                        Ok(location) => location,
                        Err(_) => continue,
                    };
                    push_unique_location(&mut candidates, &mut seen, location);
                }
            }
        }

        if candidates.is_empty() {
            let mut analysis_cache = HashMap::new();
            let mut search_matches = search_occurrences(
                ctx.root.as_path(),
                &token,
                true,
                true,
                MAX_REFERENCE_RESULTS,
                self,
            )?;
            if search_matches.is_empty() {
                search_matches = search_occurrences(
                    ctx.root.as_path(),
                    &token,
                    false,
                    true,
                    MAX_REFERENCE_RESULTS,
                    self,
                )?;
            }

            for entry in search_matches {
                let Some(declaration_kind) =
                    resolve_declaration_kind(self, &mut analysis_cache, &entry, &token)
                else {
                    continue;
                };
                let score = score_definition_candidate(ctx, req, &token, declaration_kind, &entry);
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

    pub fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
        let Some(token) = token else {
            return Ok(Vec::new());
        };

        let mut matches = search_occurrences(
            ctx.root.as_path(),
            &token,
            true,
            true,
            MAX_REFERENCE_RESULTS,
            self,
        )?;
        if matches.is_empty() {
            matches = search_occurrences(
                ctx.root.as_path(),
                &token,
                false,
                true,
                MAX_REFERENCE_RESULTS,
                self,
            )?;
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
            if is_declaration_location(self, &mut classification_cache, &location, &token) {
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
}

pub fn make_symbol(name: String, kind: &str, line: usize, column: usize) -> BasicSymbol {
    BasicSymbol {
        end_column: column + name.chars().count().saturating_sub(1),
        name,
        kind: kind.to_string(),
        line,
        column,
        end_line: line,
    }
}

fn indexed_basic_symbols(
    path: &Path,
    analyze_file: fn(&Path) -> Result<BasicFileAnalysis, String>,
) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_file(path)?;
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

pub fn find_column(line: &str, token: &str) -> Option<usize> {
    line.find(token)
        .map(|offset| line[..offset].chars().count() + 1)
}

pub fn normalize_path(path: &Path) -> PathBuf {
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

pub fn last_identifier(value: &str) -> Option<String> {
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

pub fn strip_c_style_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    let mut in_string = false;
    let mut string_delim = '\0';
    let mut escaped = false;

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
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == string_delim {
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

pub fn count_char(value: &str, needle: char) -> usize {
    value.chars().filter(|ch| *ch == needle).count()
}

pub fn strip_leading_attributes(mut line: &str, open: char, close: char) -> &str {
    loop {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(open) {
            return trimmed;
        }
        let Some(end) = find_balanced_end(trimmed, open, close) else {
            return trimmed;
        };
        line = trimmed.get(end..).unwrap_or("");
        if line.trim().is_empty() {
            return "";
        }
    }
}

pub fn find_balanced_end(value: &str, open: char, close: char) -> Option<usize> {
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

        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(index + ch.len_utf8());
            }
        }
    }

    None
}

fn search_occurrences(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
    spec: &BasicLanguageSpec,
) -> Result<Vec<BasicSearchMatch>, String> {
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
        .filter_entry(|entry| should_visit_path(entry, spec.ignored_dirs))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() || !extension_matches(entry.path(), spec.extensions) {
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
                out.push(BasicSearchMatch {
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
    symbol: &BasicSymbol,
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

fn score_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    declaration_kind: &str,
    entry: &BasicSearchMatch,
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
        "class" | "interface" | "struct" | "enum" | "record" | "object" | "namespace" => 7.0,
        "constructor" => 6.0,
        "method" | "function" => 5.0,
        "property" | "field" | "variable" | "constant" | "macro" => 3.0,
        "type" | "typedef" => 4.0,
        _ => 1.0,
    };

    if is_type_like(token)
        && matches!(
            declaration_kind,
            "class" | "interface" | "struct" | "enum" | "record" | "object" | "type" | "typedef"
        )
    {
        score += 2.0;
    }

    score
}

fn resolve_declaration_kind(
    spec: &BasicLanguageSpec,
    analysis_cache: &mut HashMap<String, Option<BasicFileAnalysis>>,
    entry: &BasicSearchMatch,
    token: &str,
) -> Option<&'static str> {
    resolve_symbol_kind(spec, analysis_cache, entry, token)
        .or_else(|| (spec.classify_declaration)(&entry.text, token))
}

fn is_declaration_location(
    spec: &BasicLanguageSpec,
    analysis_cache: &mut HashMap<String, Option<BasicFileAnalysis>>,
    location: &NavLocation,
    token: &str,
) -> bool {
    let entry = BasicSearchMatch {
        path: location.path.clone(),
        relative_path: location.relative_path.clone(),
        line: location.line,
        column: location.column,
        text: location.preview.clone(),
    };
    resolve_declaration_kind(spec, analysis_cache, &entry, token).is_some()
}

fn resolve_symbol_kind(
    spec: &BasicLanguageSpec,
    analysis_cache: &mut HashMap<String, Option<BasicFileAnalysis>>,
    entry: &BasicSearchMatch,
    token: &str,
) -> Option<&'static str> {
    let analysis = cached_analysis(spec, analysis_cache, &entry.path)?;
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

fn cached_analysis<'a>(
    spec: &BasicLanguageSpec,
    analysis_cache: &'a mut HashMap<String, Option<BasicFileAnalysis>>,
    path: &str,
) -> Option<&'a BasicFileAnalysis> {
    if !analysis_cache.contains_key(path) {
        analysis_cache.insert(path.to_string(), (spec.analyze_file)(Path::new(path)).ok());
    }
    analysis_cache.get(path).and_then(|item| item.as_ref())
}

fn declaration_kind_from_symbol_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "class" => Some("class"),
        "interface" => Some("interface"),
        "struct" => Some("struct"),
        "enum" => Some("enum"),
        "record" => Some("record"),
        "object" => Some("object"),
        "namespace" => Some("namespace"),
        "constructor" => Some("constructor"),
        "method" => Some("method"),
        "function" => Some("function"),
        "property" => Some("property"),
        "field" => Some("field"),
        "variable" => Some("variable"),
        "constant" => Some("constant"),
        "macro" => Some("macro"),
        "type" => Some("type"),
        "typedef" => Some("typedef"),
        _ => None,
    }
}

fn should_visit_path(entry: &DirEntry, ignored_dirs: &[&str]) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !ignored_dirs.contains(&name)
}

fn extension_matches(path: &Path, extensions: &[&str]) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    extensions
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
}

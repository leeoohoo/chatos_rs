// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use regex::RegexBuilder;
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::file_limits::{
    read_code_nav_file_to_string, read_code_nav_line_preview, truncate_preview,
};
use crate::services::code_nav::symbol_index::{
    nav_location_from_indexed_symbol, score_indexed_definition_candidate, IndexedSymbol,
    ProjectIndexedSymbol,
};
use crate::services::code_nav::types::{
    DocumentSymbolItem, DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities,
    NavLocation, NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;

const MAX_PREVIEW_CHARS: usize = 400;
const CODE_NAV_TEXT_SEARCH_MAX_VISITS: usize = 20_000;
const CODE_NAV_TEXT_SEARCH_DEADLINE: Duration = Duration::from_secs(3);

pub(crate) trait HeuristicNavLanguage: Send + Sync {
    type Symbol: NavSymbolLike + Send + Sync;

    const PROVIDER_ID: &'static str;
    const LANGUAGE_ID: &'static str;
    const FILE_EXTENSION: &'static str;
    const MAX_SYMBOL_RESULTS: usize;

    fn detect_project(ctx: &ProjectContext) -> bool;

    fn definition(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String>;

    fn references(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String>;

    fn analyze_document_symbols(file_path: &Path) -> Result<Vec<Self::Symbol>, String>;
}

#[async_trait::async_trait]
impl<T> CodeNavProvider for T
where
    T: HeuristicNavLanguage + 'static,
{
    fn provider_id(&self) -> &'static str {
        T::PROVIDER_ID
    }

    fn language_id(&self) -> &'static str {
        T::LANGUAGE_ID
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
        supports_extension(file_path, T::FILE_EXTENSION)
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        T::detect_project(ctx)
    }

    fn capabilities(&self, _ctx: &ProjectContext) -> NavCapabilities {
        heuristic_nav_capabilities()
    }

    async fn definition(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        let ctx = ctx.clone();
        let req = req.clone();
        tokio::task::spawn_blocking(move || T::definition(&ctx, &req))
            .await
            .map_err(|err| format!("code-nav heuristic definition task failed: {err}"))?
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        let ctx = ctx.clone();
        let req = req.clone();
        tokio::task::spawn_blocking(move || T::references(&ctx, &req))
            .await
            .map_err(|err| format!("code-nav heuristic references task failed: {err}"))?
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        let file_path = ctx.file_path.clone();
        let mode = self.document_symbols_mode();
        tokio::task::spawn_blocking(move || {
            let symbols = T::analyze_document_symbols(&file_path)?;
            Ok(document_symbols_response(
                T::PROVIDER_ID,
                T::LANGUAGE_ID,
                mode,
                &symbols,
                T::MAX_SYMBOL_RESULTS,
            ))
        })
        .await
        .map_err(|err| format!("code-nav heuristic document symbols task failed: {err}"))?
    }
}

pub(crate) fn nav_location_from_coordinates(
    root: &Path,
    path: &Path,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    score: f64,
) -> Result<Option<NavLocation>, String> {
    let preview = read_line_preview(path, line)?;
    let relative_path = pathdiff::diff_paths(path, root)
        .unwrap_or_else(|| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Some(NavLocation {
        path: normalize_path(path).to_string_lossy().to_string(),
        relative_path,
        line,
        column,
        end_line,
        end_column,
        preview,
        score,
    }))
}

pub(crate) fn push_unique_location(
    out: &mut Vec<NavLocation>,
    seen: &mut HashSet<String>,
    location: NavLocation,
) {
    let key = build_nav_key(&location);
    if seen.insert(key) {
        out.push(location);
    }
}

pub(crate) fn push_current_file_symbol_definitions<S, F>(
    root: &Path,
    file_path: &Path,
    symbols: &[S],
    token: &str,
    request_line: usize,
    score: f64,
    mut location_from_symbol: F,
    out: &mut Vec<NavLocation>,
    seen: &mut HashSet<String>,
) -> Result<(), String>
where
    S: NavSymbolLike,
    F: FnMut(&Path, &Path, &S, f64) -> Result<Option<NavLocation>, String>,
{
    for symbol in symbols
        .iter()
        .filter(|item| item.name() == token && item.line() != request_line)
    {
        if let Some(location) = location_from_symbol(root, file_path, symbol, score)? {
            push_unique_location(out, seen, location);
        }
    }
    Ok(())
}

pub(crate) fn push_indexed_definition_candidates<F>(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    indexed_symbols: &[ProjectIndexedSymbol],
    mut score_adjustment: F,
    out: &mut Vec<NavLocation>,
    seen: &mut HashSet<String>,
) where
    F: FnMut(&ProjectIndexedSymbol) -> f64,
{
    for indexed in indexed_symbols {
        if indexed.relative_path == ctx.relative_path && indexed.symbol.line == req.line {
            continue;
        }
        let score =
            score_indexed_definition_candidate(ctx, req, indexed) + score_adjustment(indexed);
        let location = match nav_location_from_indexed_symbol(&ctx.root, indexed, score) {
            Ok(location) => location,
            Err(_) => continue,
        };
        push_unique_location(out, seen, location);
    }
}

pub(crate) fn sort_and_truncate_nav_locations(
    locations: &mut Vec<NavLocation>,
    max_results: usize,
) {
    locations.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    if locations.len() > max_results {
        locations.truncate(max_results);
    }
}

pub(crate) fn search_occurrences_with_fallback<M, F>(mut search: F) -> Result<Vec<M>, String>
where
    F: FnMut(bool, bool) -> Result<Vec<M>, String>,
{
    let mut matches = search(true, true)?;
    if matches.is_empty() {
        matches = search(false, true)?;
    }
    Ok(matches)
}

pub(crate) struct TextSearchLine {
    pub(crate) searchable_text: String,
    pub(crate) preview_text: String,
}

impl TextSearchLine {
    pub(crate) fn plain(raw_line: &str) -> Self {
        let line = raw_line.trim_end_matches('\r').to_string();
        Self {
            searchable_text: line.clone(),
            preview_text: line,
        }
    }
}

pub(crate) struct TextSearchMatchParts {
    pub(crate) path: String,
    pub(crate) relative_path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) text: String,
}

pub(crate) fn search_text_occurrences<M, FileMatches, LinesForFile, BuildMatch>(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
    ignored_dirs: &[&str],
    mut file_matches: FileMatches,
    mut lines_for_file: LinesForFile,
    mut build_match: BuildMatch,
) -> Result<Vec<M>, String>
where
    FileMatches: FnMut(&Path) -> bool,
    LinesForFile: FnMut(&Path, &str) -> Vec<TextSearchLine>,
    BuildMatch: FnMut(TextSearchMatchParts) -> M,
{
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
        .filter_entry(|entry| should_visit_code_nav_path(entry, ignored_dirs))
    {
        visited_entries = visited_entries.saturating_add(1);
        ensure_code_nav_text_search_budget(started_at, visited_entries)?;

        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() || !file_matches(entry.path()) {
            continue;
        }
        let content = match read_code_nav_file_to_string(entry.path()) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let relative_path = pathdiff::diff_paths(entry.path(), root)
            .unwrap_or_else(|| entry.path().to_path_buf())
            .to_string_lossy()
            .replace('\\', "/");
        let normalized_path = normalize_path(entry.path()).to_string_lossy().to_string();
        for (index, line) in lines_for_file(entry.path(), &content)
            .into_iter()
            .enumerate()
        {
            if index % 128 == 0 {
                ensure_code_nav_text_search_budget(started_at, visited_entries)?;
            }

            for found in regex.find_iter(&line.searchable_text) {
                if out.len() >= max_results {
                    return Ok(out);
                }
                let column = line.searchable_text[..found.start()].chars().count() + 1;
                out.push(build_match(TextSearchMatchParts {
                    path: normalized_path.clone(),
                    relative_path: relative_path.clone(),
                    line: index + 1,
                    column,
                    text: truncate_preview(&line.preview_text, MAX_PREVIEW_CHARS),
                }));
            }
        }
    }

    Ok(out)
}

pub(crate) fn ensure_code_nav_text_search_budget(
    started_at: Instant,
    visited_entries: usize,
) -> Result<(), String> {
    if visited_entries > CODE_NAV_TEXT_SEARCH_MAX_VISITS {
        return Err(format!(
            "code-nav text search exceeded {CODE_NAV_TEXT_SEARCH_MAX_VISITS} entries"
        ));
    }
    if started_at.elapsed() >= CODE_NAV_TEXT_SEARCH_DEADLINE {
        return Err(format!(
            "code-nav text search exceeded {:?}",
            CODE_NAV_TEXT_SEARCH_DEADLINE
        ));
    }
    Ok(())
}

pub(crate) fn select_reference_locations<M, F>(
    ctx: &ProjectContext,
    _req: &NavPositionRequest,
    token: &str,
    matches: Vec<M>,
    max_results: usize,
    mut is_declaration: F,
) -> Vec<NavLocation>
where
    M: NavSearchMatchLike,
    F: FnMut(&NavLocation, &str) -> bool,
{
    let mut locations = Vec::new();
    let mut seen = HashSet::new();
    for entry in matches {
        let score = if entry.relative_path() == ctx.relative_path {
            1.5
        } else {
            1.0
        };
        let location = nav_location_from_search_match(token, &entry, score);
        push_unique_location(&mut locations, &mut seen, location);
    }

    let mut declarations = Vec::new();
    let mut references = Vec::new();
    for location in locations {
        if is_declaration(&location, token) {
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
    sort_reference_locations(&mut out, &ctx.relative_path, max_results);
    out
}

pub(crate) fn push_definition_search_matches<M, D, S>(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    matches: Vec<M>,
    mut declaration_kind: D,
    mut score_candidate: S,
    out: &mut Vec<NavLocation>,
    seen: &mut HashSet<String>,
) where
    M: NavSearchMatchLike,
    D: FnMut(&M, &str) -> Option<&'static str>,
    S: FnMut(&M, &str, &'static str) -> f64,
{
    for entry in matches {
        let Some(declaration_kind) = declaration_kind(&entry, token) else {
            continue;
        };
        let score = score_candidate(&entry, token, declaration_kind);
        let location = nav_location_from_search_match(token, &entry, score);
        if is_request_token_location(ctx, req, token, &location) {
            continue;
        }
        push_unique_location(out, seen, location);
    }
}

pub(crate) fn is_request_token_location(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    location: &NavLocation,
) -> bool {
    if location.line != req.line {
        return false;
    }
    let same_file = location.relative_path == ctx.relative_path
        || normalize_path(Path::new(&location.path)) == normalize_path(&ctx.file_path);
    if !same_file {
        return false;
    }

    let token_width = token.chars().count().max(1);
    let location_end = location
        .end_column
        .max(location.column + token_width.saturating_sub(1));
    req.column >= location.column && req.column <= location_end
}

pub(crate) trait NavSymbolLike {
    fn name(&self) -> &str;
    fn kind(&self) -> &str;
    fn line(&self) -> usize;
    fn column(&self) -> usize;
    fn end_line(&self) -> usize;
    fn end_column(&self) -> usize;
}

pub(crate) trait NavSearchMatchLike {
    fn path(&self) -> &str;
    fn relative_path(&self) -> &str;
    fn line(&self) -> usize;
    fn column(&self) -> usize;
    fn text(&self) -> &str;
}

macro_rules! impl_nav_symbol_like_for_field_struct {
    ($ty:ty) => {
        impl $crate::services::code_nav::languages::shared_nav::NavSymbolLike for $ty {
            fn name(&self) -> &str {
                &self.name
            }

            fn kind(&self) -> &str {
                &self.kind
            }

            fn line(&self) -> usize {
                self.line
            }

            fn column(&self) -> usize {
                self.column
            }

            fn end_line(&self) -> usize {
                self.end_line
            }

            fn end_column(&self) -> usize {
                self.end_column
            }
        }
    };
}

macro_rules! impl_nav_search_match_like_for_field_struct {
    ($ty:ty) => {
        impl $crate::services::code_nav::languages::shared_nav::NavSearchMatchLike for $ty {
            fn path(&self) -> &str {
                &self.path
            }

            fn relative_path(&self) -> &str {
                &self.relative_path
            }

            fn line(&self) -> usize {
                self.line
            }

            fn column(&self) -> usize {
                self.column
            }

            fn text(&self) -> &str {
                &self.text
            }
        }
    };
}

pub(crate) use impl_nav_search_match_like_for_field_struct;
pub(crate) use impl_nav_symbol_like_for_field_struct;

pub(crate) fn indexed_symbols_from<S: NavSymbolLike>(symbols: &[S]) -> Vec<IndexedSymbol> {
    symbols
        .iter()
        .map(|symbol| IndexedSymbol {
            name: symbol.name().to_string(),
            kind: symbol.kind().to_string(),
            line: symbol.line(),
            column: symbol.column(),
            end_line: symbol.end_line(),
            end_column: symbol.end_column(),
        })
        .collect()
}

pub(crate) fn document_symbols_response<S: NavSymbolLike>(
    provider: &str,
    language: &str,
    mode: &str,
    symbols: &[S],
    max_symbols: usize,
) -> DocumentSymbolsResponse {
    let mut symbols: Vec<DocumentSymbolItem> = symbols
        .iter()
        .map(|item| DocumentSymbolItem {
            name: item.name().to_string(),
            kind: item.kind().to_string(),
            line: item.line(),
            column: item.column(),
            end_line: item.end_line(),
            end_column: item.end_column(),
        })
        .collect();
    if symbols.len() > max_symbols {
        symbols.truncate(max_symbols);
    }

    DocumentSymbolsResponse {
        provider: provider.to_string(),
        language: language.to_string(),
        mode: mode.to_string(),
        symbols,
    }
}

pub(crate) fn heuristic_nav_capabilities() -> NavCapabilities {
    NavCapabilities {
        supports_definition: true,
        supports_references: true,
        supports_document_symbols: true,
    }
}

pub(crate) fn supports_extension(file_path: &Path, extension: &str) -> bool {
    file_path.extension().and_then(|value| value.to_str()) == Some(extension)
}

fn sort_reference_locations(
    locations: &mut Vec<NavLocation>,
    current_relative_path: &str,
    max_results: usize,
) {
    locations.sort_by(|left, right| {
        (left.relative_path != current_relative_path)
            .cmp(&(right.relative_path != current_relative_path))
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    if locations.len() > max_results {
        locations.truncate(max_results);
    }
}

fn should_visit_code_nav_path(entry: &DirEntry, ignored_dirs: &[&str]) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !ignored_dirs.contains(&name)
}

fn nav_location_from_search_match<M: NavSearchMatchLike>(
    token: &str,
    entry: &M,
    score: f64,
) -> NavLocation {
    NavLocation {
        path: entry.path().to_string(),
        relative_path: entry.relative_path().to_string(),
        line: entry.line(),
        column: entry.column(),
        end_line: entry.line(),
        end_column: entry.column() + token.chars().count().saturating_sub(1),
        preview: entry.text().to_string(),
        score,
    }
}

pub(crate) fn declaration_kind_from_symbol_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "class" => Some("class"),
        "interface" => Some("interface"),
        "struct" => Some("struct"),
        "enum" => Some("enum"),
        "record" => Some("record"),
        "object" => Some("object"),
        "namespace" => Some("namespace"),
        "constructor" => Some("constructor"),
        "trait" => Some("trait"),
        "module" => Some("module"),
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

pub(crate) fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
}

pub(crate) fn find_column(line: &str, token: &str) -> Option<usize> {
    line.find(token)
        .map(|offset| line[..offset].chars().count() + 1)
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
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

pub(crate) fn count_char(value: &str, needle: char) -> usize {
    value.chars().filter(|ch| *ch == needle).count()
}

pub(crate) fn last_identifier(value: &str) -> Option<String> {
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

fn read_line_preview(path: &Path, line: usize) -> Result<String, String> {
    read_code_nav_line_preview(path, line, MAX_PREVIEW_CHARS)
}

fn build_nav_key(location: &NavLocation) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        location.path, location.line, location.column, location.end_line, location.end_column
    )
}

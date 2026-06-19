use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::languages::shared_nav::{
    push_current_file_symbol_definitions, push_definition_search_matches,
    push_indexed_definition_candidates, search_occurrences_with_fallback,
    select_reference_locations, sort_and_truncate_nav_locations,
};
use crate::services::code_nav::symbol_index::project_symbol_index;
use crate::services::code_nav::types::{NavLocation, NavPositionRequest, ProjectContext};

use super::search::{
    indexed_basic_symbols, nav_location_from_symbol, search_occurrences, BasicSearchMatch,
};
use super::{BasicFileAnalysis, BasicLanguageSpec};

const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;

pub(super) fn find_definitions(
    spec: &BasicLanguageSpec,
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = (spec.analyze_file)(&ctx.file_path)?;
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    push_current_file_symbol_definitions(
        &ctx.root,
        &ctx.file_path,
        &current.symbols,
        &token,
        req.line,
        9.0,
        nav_location_from_symbol,
        &mut candidates,
        &mut seen,
    )?;

    if let Ok(index) = project_symbol_index(
        ctx.root.as_path(),
        spec.provider_id,
        spec.extensions,
        spec.ignored_dirs,
        |path| indexed_basic_symbols(path, spec.analyze_file),
    ) {
        if let Some(symbols) = index.symbols_by_name.get(&token) {
            push_indexed_definition_candidates(
                ctx,
                req,
                symbols,
                |_| 0.0,
                &mut candidates,
                &mut seen,
            );
        }
    }

    if candidates.is_empty() {
        let mut analysis_cache = HashMap::new();
        let search_matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
            search_occurrences(
                ctx.root.as_path(),
                &token,
                case_sensitive,
                whole_word,
                MAX_REFERENCE_RESULTS,
                spec,
            )
        })?;

        push_definition_search_matches(
            ctx,
            req,
            &token,
            search_matches,
            |entry, token| resolve_declaration_kind(spec, &mut analysis_cache, entry, token),
            |entry, token, declaration_kind| {
                score_definition_candidate(ctx, req, token, declaration_kind, entry)
            },
            &mut candidates,
            &mut seen,
        );
    }

    sort_and_truncate_nav_locations(&mut candidates, MAX_DEFINITION_RESULTS);

    Ok(candidates)
}

pub(super) fn find_references(
    spec: &BasicLanguageSpec,
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
        search_occurrences(
            ctx.root.as_path(),
            &token,
            case_sensitive,
            whole_word,
            MAX_REFERENCE_RESULTS,
            spec,
        )
    })?;
    let mut classification_cache = HashMap::new();
    Ok(select_reference_locations(
        ctx,
        req,
        &token,
        matches,
        MAX_REFERENCE_RESULTS,
        |location, token| is_declaration_location(spec, &mut classification_cache, location, token),
    ))
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
    crate::services::code_nav::languages::shared_nav::declaration_kind_from_symbol_kind(kind)
}

fn is_type_like(token: &str) -> bool {
    crate::services::code_nav::languages::shared_nav::is_type_like(token)
}

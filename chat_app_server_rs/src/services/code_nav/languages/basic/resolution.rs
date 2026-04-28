use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::symbol_index::{
    nav_location_from_indexed_symbol, project_symbol_index, score_indexed_definition_candidate,
};
use crate::services::code_nav::types::{NavLocation, NavPositionRequest, ProjectContext};

use super::search::{
    indexed_basic_symbols, nav_location_from_symbol, push_unique_location, search_occurrences,
    BasicSearchMatch,
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
        ctx.root.as_path(),
        spec.provider_id,
        spec.extensions,
        spec.ignored_dirs,
        |path| indexed_basic_symbols(path, spec.analyze_file),
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
        let mut search_matches = search_occurrences(
            ctx.root.as_path(),
            &token,
            true,
            true,
            MAX_REFERENCE_RESULTS,
            spec,
        )?;
        if search_matches.is_empty() {
            search_matches = search_occurrences(
                ctx.root.as_path(),
                &token,
                false,
                true,
                MAX_REFERENCE_RESULTS,
                spec,
            )?;
        }

        for entry in search_matches {
            let Some(declaration_kind) =
                resolve_declaration_kind(spec, &mut analysis_cache, &entry, &token)
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

pub(super) fn find_references(
    spec: &BasicLanguageSpec,
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
        spec,
    )?;
    if matches.is_empty() {
        matches = search_occurrences(
            ctx.root.as_path(),
            &token,
            false,
            true,
            MAX_REFERENCE_RESULTS,
            spec,
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
        push_unique_location(&mut locations, &mut seen, location);
    }

    let mut declarations = Vec::new();
    let mut references = Vec::new();
    let mut classification_cache = HashMap::new();
    for location in locations {
        if is_declaration_location(spec, &mut classification_cache, &location, &token) {
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

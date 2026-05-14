use std::fs;
use std::path::Path;

use regex::RegexBuilder;
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::languages::shared_nav::nav_location_from_coordinates;
pub(super) use crate::services::code_nav::languages::shared_nav::push_unique_location;
use crate::services::code_nav::symbol_index::IndexedSymbol;
use crate::services::code_nav::types::NavLocation;

use super::helpers::{extension_matches, normalize_path};
use super::{BasicFileAnalysis, BasicLanguageSpec, BasicSymbol};

const MAX_PREVIEW_CHARS: usize = 400;

#[derive(Debug, Clone)]
pub(super) struct BasicSearchMatch {
    pub(super) path: String,
    pub(super) relative_path: String,
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) text: String,
}

pub(super) fn indexed_basic_symbols(
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

pub(super) fn search_occurrences(
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
                    text: if normalized_line.len() > MAX_PREVIEW_CHARS {
                        normalized_line[..MAX_PREVIEW_CHARS].to_string()
                    } else {
                        normalized_line.to_string()
                    },
                });
            }
        }
    }

    Ok(out)
}

pub(super) fn nav_location_from_symbol(
    root: &Path,
    path: &Path,
    symbol: &BasicSymbol,
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

fn should_visit_path(entry: &DirEntry, ignored_dirs: &[&str]) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !ignored_dirs.contains(&name)
}

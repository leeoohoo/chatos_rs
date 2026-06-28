use std::path::Path;
use std::time::Instant;

use regex::RegexBuilder;
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::file_limits::{read_code_nav_file_to_string, truncate_preview};
use crate::services::code_nav::languages::shared_nav::{
    ensure_code_nav_text_search_budget, indexed_symbols_from, nav_location_from_coordinates,
    NavSearchMatchLike,
};
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

impl NavSearchMatchLike for BasicSearchMatch {
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

pub(super) fn indexed_basic_symbols(
    path: &Path,
    analyze_file: fn(&Path) -> Result<BasicFileAnalysis, String>,
) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_file(path)?;
    Ok(indexed_symbols_from(&analysis.symbols))
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
    let started_at = Instant::now();
    let mut visited_entries = 0usize;

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_path(entry, spec.ignored_dirs))
    {
        visited_entries = visited_entries.saturating_add(1);
        ensure_code_nav_text_search_budget(started_at, visited_entries)?;

        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() || !extension_matches(entry.path(), spec.extensions) {
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
                out.push(BasicSearchMatch {
                    path: normalize_path(entry.path()).to_string_lossy().to_string(),
                    relative_path,
                    line: index + 1,
                    column,
                    text: truncate_preview(normalized_line, MAX_PREVIEW_CHARS),
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

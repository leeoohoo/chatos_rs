// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use crate::services::code_nav::languages::shared_nav::{
    indexed_symbols_from, nav_location_from_coordinates, search_text_occurrences,
    NavSearchMatchLike, TextSearchLine, TextSearchMatchParts,
};
use crate::services::code_nav::symbol_index::IndexedSymbol;
use crate::services::code_nav::types::NavLocation;

use super::helpers::extension_matches;
use super::{BasicFileAnalysis, BasicLanguageSpec, BasicSymbol};

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
    search_text_occurrences(
        root,
        query,
        case_sensitive,
        whole_word,
        max_results,
        spec.ignored_dirs,
        |path| extension_matches(path, spec.extensions),
        |_path, content| content.lines().map(TextSearchLine::plain).collect(),
        |parts: TextSearchMatchParts| BasicSearchMatch {
            path: parts.path,
            relative_path: parts.relative_path,
            line: parts.line,
            column: parts.column,
            text: parts.text,
        },
    )
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

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use crate::services::code_nav::languages::shared_nav::{
    impl_nav_search_match_like_for_field_struct, search_text_occurrences, TextSearchLine,
    TextSearchMatchParts,
};

use super::RUST_IGNORED_DIRS;

#[derive(Debug, Clone)]
pub(super) struct RustSearchMatch {
    pub(super) path: String,
    pub(super) relative_path: String,
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) text: String,
}

impl_nav_search_match_like_for_field_struct!(RustSearchMatch);

pub(super) fn search_rust_occurrences(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_results: usize,
) -> Result<Vec<RustSearchMatch>, String> {
    search_text_occurrences(
        root,
        query,
        case_sensitive,
        whole_word,
        max_results,
        RUST_IGNORED_DIRS,
        |path| path.extension().and_then(|value| value.to_str()) == Some("rs"),
        |_path, content| content.lines().map(TextSearchLine::plain).collect(),
        |parts: TextSearchMatchParts| RustSearchMatch {
            path: parts.path,
            relative_path: parts.relative_path,
            line: parts.line,
            column: parts.column,
            text: parts.text,
        },
    )
}

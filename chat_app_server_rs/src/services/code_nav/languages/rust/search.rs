use std::path::Path;
use std::time::Instant;

use regex::RegexBuilder;
use walkdir::{DirEntry, WalkDir};

use crate::services::code_nav::file_limits::{read_code_nav_file_to_string, truncate_preview};
use crate::services::code_nav::languages::shared_nav::{
    ensure_code_nav_text_search_budget, impl_nav_search_match_like_for_field_struct, normalize_path,
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
        .filter_entry(|entry| should_visit_rust_path(entry))
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
        if entry.path().extension().and_then(|value| value.to_str()) != Some("rs") {
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
                out.push(RustSearchMatch {
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

fn should_visit_rust_path(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !RUST_IGNORED_DIRS.contains(&name)
}

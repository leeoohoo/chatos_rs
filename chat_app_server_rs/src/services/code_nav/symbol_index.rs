use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use walkdir::WalkDir;

use super::types::{NavLocation, NavPositionRequest, ProjectContext};
use crate::services::project_local_cache::{read_cache_json, write_cache_json};

mod files;
mod persistence;

#[cfg(test)]
mod tests;

use files::{
    extension_matches, fingerprint_symbol_file, normalize_path, read_line_preview,
    should_visit_path, ProjectSymbolIndexSnapshot,
};
use persistence::{
    from_persisted_index, persisted_project_symbol_index_entry, symbol_index_cache_path,
    PersistedProjectSymbolIndexEntry,
};

static PROJECT_SYMBOL_INDEX_CACHE: Lazy<DashMap<String, ProjectSymbolIndexCacheEntry>> =
    Lazy::new(DashMap::new);
static PROJECT_SYMBOL_INDEX_DIRTY_PATHS: Lazy<DashMap<String, Vec<PathBuf>>> =
    Lazy::new(DashMap::new);

const PROJECT_SYMBOL_INDEX_MAX_VISITS: usize = 20_000;
const PROJECT_SYMBOL_INDEX_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct IndexedSymbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone)]
pub struct ProjectIndexedSymbol {
    pub path: String,
    pub relative_path: String,
    pub symbol: IndexedSymbol,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectSymbolIndex {
    pub symbols_by_name: HashMap<String, Vec<ProjectIndexedSymbol>>,
}

#[derive(Debug, Clone)]
struct ProjectSymbolIndexCacheEntry {
    root: PathBuf,
    snapshot: ProjectSymbolIndexSnapshot,
    index: ProjectSymbolIndex,
}

pub fn project_symbol_index(
    root: &Path,
    provider_id: &str,
    extensions: &[&str],
    ignored_dirs: &[&str],
    analyze_file: impl Fn(&Path) -> Result<Vec<IndexedSymbol>, String>,
) -> Result<ProjectSymbolIndex, String> {
    let key = project_symbol_index_cache_key(root, provider_id);
    if let Some(entry) = PROJECT_SYMBOL_INDEX_CACHE
        .get(&key)
        .map(|entry| entry.value().clone())
    {
        if let Some((_, dirty_paths)) = PROJECT_SYMBOL_INDEX_DIRTY_PATHS.remove(&key) {
            let current_snapshot = project_symbol_index_snapshot(root, extensions, ignored_dirs)?;
            let rebuilt = rebuild_project_symbol_index_for_dirty_paths(
                root,
                &entry.snapshot,
                &entry.index,
                &current_snapshot,
                dirty_paths.as_slice(),
                &analyze_file,
            )?;
            let _ = write_cache_json(
                root.to_string_lossy().as_ref(),
                symbol_index_cache_path(provider_id).as_str(),
                &persisted_project_symbol_index_entry(rebuilt.1.clone(), &rebuilt.0),
            );
            PROJECT_SYMBOL_INDEX_CACHE.insert(
                key.clone(),
                ProjectSymbolIndexCacheEntry {
                    root: normalize_path(root),
                    snapshot: rebuilt.1.clone(),
                    index: rebuilt.0.clone(),
                },
            );
            return Ok(rebuilt.0);
        }
        let current_snapshot = project_symbol_index_snapshot(root, extensions, ignored_dirs)?;
        if entry.snapshot == current_snapshot {
            return Ok(entry.index.clone());
        }
    }

    if let Some(persisted) = read_cache_json::<PersistedProjectSymbolIndexEntry>(
        root.to_string_lossy().as_ref(),
        symbol_index_cache_path(provider_id).as_str(),
    )? {
        let current_snapshot = project_symbol_index_snapshot(root, extensions, ignored_dirs)?;
        if persisted.snapshot == current_snapshot {
            let index = from_persisted_index(persisted.index);
            PROJECT_SYMBOL_INDEX_CACHE.insert(
                key,
                ProjectSymbolIndexCacheEntry {
                    root: normalize_path(root),
                    snapshot: current_snapshot,
                    index: index.clone(),
                },
            );
            return Ok(index);
        }
    }

    let (index, snapshot) =
        build_project_symbol_index(root, extensions, ignored_dirs, &analyze_file)?;
    let _ = write_cache_json(
        root.to_string_lossy().as_ref(),
        symbol_index_cache_path(provider_id).as_str(),
        &persisted_project_symbol_index_entry(snapshot.clone(), &index),
    );
    PROJECT_SYMBOL_INDEX_CACHE.insert(
        key,
        ProjectSymbolIndexCacheEntry {
            root: normalize_path(root),
            snapshot,
            index: index.clone(),
        },
    );
    Ok(index)
}

pub fn invalidate_project_symbol_indexes_for_path(path: &Path) -> usize {
    let target = normalize_path(path);
    let keys: Vec<String> = PROJECT_SYMBOL_INDEX_CACHE
        .iter()
        .filter_map(|entry| {
            let root = entry.value().root.as_path();
            if target.starts_with(root) || root.starts_with(target.as_path()) {
                Some(entry.key().clone())
            } else {
                None
            }
        })
        .collect();
    for key in keys {
        PROJECT_SYMBOL_INDEX_DIRTY_PATHS
            .entry(key)
            .and_modify(|paths| {
                if !paths.iter().any(|item| item == &target) {
                    paths.push(target.clone());
                }
            })
            .or_insert_with(|| vec![target.clone()]);
    }
    PROJECT_SYMBOL_INDEX_DIRTY_PATHS.len()
}

pub fn nav_location_from_indexed_symbol(
    root: &Path,
    indexed: &ProjectIndexedSymbol,
    score: f64,
) -> Result<NavLocation, String> {
    let path = Path::new(indexed.path.as_str());
    let preview = read_line_preview(path, indexed.symbol.line)?;
    let relative_path = pathdiff::diff_paths(path, root)
        .unwrap_or_else(|| PathBuf::from(indexed.relative_path.as_str()))
        .to_string_lossy()
        .replace('\\', "/");

    Ok(NavLocation {
        path: normalize_path(path).to_string_lossy().to_string(),
        relative_path,
        line: indexed.symbol.line,
        column: indexed.symbol.column,
        end_line: indexed.symbol.end_line,
        end_column: indexed.symbol.end_column,
        preview,
        score,
    })
}

pub fn score_indexed_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    indexed: &ProjectIndexedSymbol,
) -> f64 {
    let mut score = 0.0;
    let is_same_file = indexed.relative_path == ctx.relative_path;
    let is_same_line = is_same_file && indexed.symbol.line == req.line;
    let file_stem = Path::new(&indexed.relative_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if file_stem == indexed.symbol.name {
        score += 4.0;
    }
    if is_same_file {
        score += 2.0;
    }
    if is_same_line {
        score -= 4.0;
    }

    score += match indexed.symbol.kind.as_str() {
        "class" | "interface" | "struct" | "enum" | "record" | "object" | "namespace" => 7.0,
        "constructor" => 6.0,
        "method" | "function" => 5.0,
        "property" | "field" | "variable" | "constant" | "macro" => 3.0,
        "type" | "typedef" => 4.0,
        _ => 1.0,
    };

    if is_type_like(indexed.symbol.name.as_str())
        && matches!(
            indexed.symbol.kind.as_str(),
            "class" | "interface" | "struct" | "enum" | "record" | "object" | "type" | "typedef"
        )
    {
        score += 2.0;
    }

    score
}

fn build_project_symbol_index(
    root: &Path,
    extensions: &[&str],
    ignored_dirs: &[&str],
    analyze_file: &impl Fn(&Path) -> Result<Vec<IndexedSymbol>, String>,
) -> Result<(ProjectSymbolIndex, ProjectSymbolIndexSnapshot), String> {
    let started_at = Instant::now();
    let mut visited_entries = 0usize;
    let mut index = ProjectSymbolIndex::default();
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_path(entry, ignored_dirs))
    {
        visited_entries = visited_entries.saturating_add(1);
        ensure_symbol_index_scan_budget(started_at, visited_entries)?;

        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() || !extension_matches(entry.path(), extensions) {
            continue;
        }

        let Some(fingerprint) = fingerprint_symbol_file(root, entry.path()) else {
            continue;
        };
        files.push(fingerprint.clone());
        let path = normalize_path(entry.path());
        let symbols = match analyze_file(path.as_path()) {
            Ok(symbols) => symbols,
            Err(_) => continue,
        };
        let relative_path = fingerprint.relative_path.clone();
        let path_text = path.to_string_lossy().to_string();

        for symbol in symbols {
            index
                .symbols_by_name
                .entry(symbol.name.clone())
                .or_default()
                .push(ProjectIndexedSymbol {
                    path: path_text.clone(),
                    relative_path: relative_path.clone(),
                    symbol,
                });
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok((index, ProjectSymbolIndexSnapshot { files }))
}

fn rebuild_project_symbol_index_for_dirty_paths(
    root: &Path,
    previous_snapshot: &ProjectSymbolIndexSnapshot,
    previous_index: &ProjectSymbolIndex,
    current_snapshot: &ProjectSymbolIndexSnapshot,
    dirty_paths: &[PathBuf],
    analyze_file: &impl Fn(&Path) -> Result<Vec<IndexedSymbol>, String>,
) -> Result<(ProjectSymbolIndex, ProjectSymbolIndexSnapshot), String> {
    let dirty_relative_paths = dirty_paths
        .iter()
        .filter_map(|path| {
            pathdiff::diff_paths(path, root).map(|value| value.to_string_lossy().replace('\\', "/"))
        })
        .collect::<Vec<_>>();
    if dirty_relative_paths.is_empty() {
        return Ok((previous_index.clone(), current_snapshot.clone()));
    }

    let dirty_set = dirty_relative_paths
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let current_fingerprint_by_path = current_snapshot
        .files
        .iter()
        .cloned()
        .map(|item| (item.relative_path.clone(), item))
        .collect::<HashMap<_, _>>();

    let mut next_index = ProjectSymbolIndex::default();
    for (name, items) in &previous_index.symbols_by_name {
        let retained = items
            .iter()
            .filter(|item| !dirty_set.contains(&item.relative_path))
            .cloned()
            .collect::<Vec<_>>();
        if !retained.is_empty() {
            next_index.symbols_by_name.insert(name.clone(), retained);
        }
    }

    for relative_path in dirty_set {
        let Some(_fingerprint) = current_fingerprint_by_path.get(&relative_path) else {
            continue;
        };
        let absolute_path = root.join(relative_path.as_str());
        let path = normalize_path(absolute_path.as_path());
        let symbols = match analyze_file(path.as_path()) {
            Ok(symbols) => symbols,
            Err(_) => continue,
        };
        let path_text = path.to_string_lossy().to_string();
        for symbol in symbols {
            next_index
                .symbols_by_name
                .entry(symbol.name.clone())
                .or_default()
                .push(ProjectIndexedSymbol {
                    path: path_text.clone(),
                    relative_path: relative_path.clone(),
                    symbol,
                });
        }
    }

    let _ = previous_snapshot;
    Ok((next_index, current_snapshot.clone()))
}

fn project_symbol_index_cache_key(root: &Path, provider_id: &str) -> String {
    format!("{}:{}", provider_id, normalize_path(root).to_string_lossy())
}

fn project_symbol_index_snapshot(
    root: &Path,
    extensions: &[&str],
    ignored_dirs: &[&str],
) -> Result<ProjectSymbolIndexSnapshot, String> {
    let started_at = Instant::now();
    let mut visited_entries = 0usize;
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_path(entry, ignored_dirs))
    {
        visited_entries = visited_entries.saturating_add(1);
        ensure_symbol_index_scan_budget(started_at, visited_entries)?;

        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() || !extension_matches(entry.path(), extensions) {
            continue;
        }
        if let Some(fingerprint) = fingerprint_symbol_file(root, entry.path()) {
            files.push(fingerprint);
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(ProjectSymbolIndexSnapshot { files })
}

fn ensure_symbol_index_scan_budget(
    started_at: Instant,
    visited_entries: usize,
) -> Result<(), String> {
    if visited_entries > PROJECT_SYMBOL_INDEX_MAX_VISITS {
        return Err(format!(
            "code-nav symbol index scan exceeded {PROJECT_SYMBOL_INDEX_MAX_VISITS} entries"
        ));
    }
    if started_at.elapsed() >= PROJECT_SYMBOL_INDEX_DEADLINE {
        return Err(format!(
            "code-nav symbol index scan exceeded {:?}",
            PROJECT_SYMBOL_INDEX_DEADLINE
        ));
    }
    Ok(())
}

fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
}

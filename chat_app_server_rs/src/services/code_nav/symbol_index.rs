use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use walkdir::{DirEntry, WalkDir};

use super::types::{NavLocation, NavPositionRequest, ProjectContext};

static PROJECT_SYMBOL_INDEX_CACHE: Lazy<DashMap<String, ProjectSymbolIndexCacheEntry>> =
    Lazy::new(DashMap::new);

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSymbolIndexSnapshot {
    files: Vec<ProjectSymbolFileFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSymbolFileFingerprint {
    relative_path: String,
    size: u64,
    modified_unix_nanos: u128,
}

pub fn project_symbol_index(
    root: &Path,
    provider_id: &str,
    extensions: &[&str],
    ignored_dirs: &[&str],
    analyze_file: impl Fn(&Path) -> Result<Vec<IndexedSymbol>, String>,
) -> Result<ProjectSymbolIndex, String> {
    let key = project_symbol_index_cache_key(root, provider_id);
    if let Some(entry) = PROJECT_SYMBOL_INDEX_CACHE.get(&key) {
        let current_snapshot = project_symbol_index_snapshot(root, extensions, ignored_dirs)?;
        if entry.snapshot == current_snapshot {
            return Ok(entry.index.clone());
        }
    }

    let (index, snapshot) =
        build_project_symbol_index(root, extensions, ignored_dirs, &analyze_file)?;
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
    let removed = keys.len();
    for key in keys {
        PROJECT_SYMBOL_INDEX_CACHE.remove(&key);
    }
    removed
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
    let mut index = ProjectSymbolIndex::default();
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_path(entry, ignored_dirs))
    {
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

fn project_symbol_index_cache_key(root: &Path, provider_id: &str) -> String {
    format!("{}:{}", provider_id, normalize_path(root).to_string_lossy())
}

fn project_symbol_index_snapshot(
    root: &Path,
    extensions: &[&str],
    ignored_dirs: &[&str],
) -> Result<ProjectSymbolIndexSnapshot, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_visit_path(entry, ignored_dirs))
    {
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

fn fingerprint_symbol_file(root: &Path, path: &Path) -> Option<ProjectSymbolFileFingerprint> {
    let metadata = fs::metadata(path).ok()?;
    let normalized_path = normalize_path(path);
    let relative_path = pathdiff::diff_paths(normalized_path.as_path(), root)
        .unwrap_or_else(|| normalized_path.clone())
        .to_string_lossy()
        .replace('\\', "/");
    Some(ProjectSymbolFileFingerprint {
        relative_path,
        size: metadata.len(),
        modified_unix_nanos: metadata
            .modified()
            .ok()
            .map(system_time_to_unix_nanos)
            .unwrap_or(0),
    })
}

fn system_time_to_unix_nanos(value: SystemTime) -> u128 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos()
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

fn normalize_path(path: &Path) -> PathBuf {
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

fn is_type_like(token: &str) -> bool {
    token
        .chars()
        .next()
        .map(|value| value.is_uppercase())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        invalidate_project_symbol_indexes_for_path, nav_location_from_indexed_symbol,
        project_symbol_index, IndexedSymbol,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn make_temp_symbol_index_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_symbol_index_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("src")).expect("create source dir");
        root
    }

    fn analyze_fixture_file(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
        let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let mut symbols = Vec::new();
        for (index, line) in content.lines().enumerate() {
            let Some(name) = line.strip_prefix("symbol ") else {
                continue;
            };
            let name = name.trim().to_string();
            let column = line.find(&name).unwrap_or(0) + 1;
            symbols.push(IndexedSymbol {
                end_column: column + name.chars().count().saturating_sub(1),
                name,
                kind: "function".to_string(),
                line: index + 1,
                column,
                end_line: index + 1,
            });
        }
        Ok(symbols)
    }

    #[test]
    fn project_symbol_index_collects_symbols_and_builds_locations() {
        let root = make_temp_symbol_index_project();
        let path = root.join("src/main.demo");
        fs::write(&path, "symbol greet\ncall greet\n").expect("write fixture");

        let index = project_symbol_index(
            root.as_path(),
            "test-symbol-index",
            &["demo"],
            &["ignored"],
            analyze_fixture_file,
        )
        .expect("build project symbol index");

        let symbols = index
            .symbols_by_name
            .get("greet")
            .expect("greet should be indexed");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].relative_path, "src/main.demo");

        let location =
            nav_location_from_indexed_symbol(root.as_path(), &symbols[0], 7.0).expect("location");
        assert_eq!(location.line, 1);
        assert_eq!(location.column, 8);
        assert_eq!(location.preview, "symbol greet");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn project_symbol_index_reuses_cache_while_source_snapshot_is_unchanged() {
        let root = make_temp_symbol_index_project();
        let path = root.join("src/main.demo");
        fs::write(&path, "symbol greet\n").expect("write fixture");
        let analyze_calls = Arc::new(AtomicUsize::new(0));
        let provider_id = format!("test-symbol-index-cache-{}", uuid::Uuid::new_v4());

        let first_counter = Arc::clone(&analyze_calls);
        project_symbol_index(
            root.as_path(),
            provider_id.as_str(),
            &["demo"],
            &["ignored"],
            move |path| {
                first_counter.fetch_add(1, Ordering::SeqCst);
                analyze_fixture_file(path)
            },
        )
        .expect("build project symbol index");

        let second_counter = Arc::clone(&analyze_calls);
        project_symbol_index(
            root.as_path(),
            provider_id.as_str(),
            &["demo"],
            &["ignored"],
            move |path| {
                second_counter.fetch_add(1, Ordering::SeqCst);
                analyze_fixture_file(path)
            },
        )
        .expect("reuse project symbol index");

        assert_eq!(analyze_calls.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn project_symbol_index_rebuilds_when_source_snapshot_changes() {
        let root = make_temp_symbol_index_project();
        let path = root.join("src/main.demo");
        fs::write(&path, "symbol greet\n").expect("write fixture");
        let provider_id = format!("test-symbol-index-refresh-{}", uuid::Uuid::new_v4());

        project_symbol_index(
            root.as_path(),
            provider_id.as_str(),
            &["demo"],
            &["ignored"],
            analyze_fixture_file,
        )
        .expect("build project symbol index");

        fs::write(&path, "symbol greet\nsymbol farewell\n").expect("update fixture");
        let index = project_symbol_index(
            root.as_path(),
            provider_id.as_str(),
            &["demo"],
            &["ignored"],
            analyze_fixture_file,
        )
        .expect("refresh project symbol index");

        assert!(index.symbols_by_name.contains_key("farewell"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn project_symbol_index_can_be_invalidated_by_changed_path() {
        let root = make_temp_symbol_index_project();
        let path = root.join("src/main.demo");
        fs::write(&path, "symbol greet\n").expect("write fixture");
        let analyze_calls = Arc::new(AtomicUsize::new(0));
        let provider_id = format!("test-symbol-index-invalidate-{}", uuid::Uuid::new_v4());

        let first_counter = Arc::clone(&analyze_calls);
        project_symbol_index(
            root.as_path(),
            provider_id.as_str(),
            &["demo"],
            &["ignored"],
            move |path| {
                first_counter.fetch_add(1, Ordering::SeqCst);
                analyze_fixture_file(path)
            },
        )
        .expect("build project symbol index");

        let removed = invalidate_project_symbol_indexes_for_path(path.as_path());
        assert_eq!(removed, 1);

        let second_counter = Arc::clone(&analyze_calls);
        project_symbol_index(
            root.as_path(),
            provider_id.as_str(),
            &["demo"],
            &["ignored"],
            move |path| {
                second_counter.fetch_add(1, Ordering::SeqCst);
                analyze_fixture_file(path)
            },
        )
        .expect("rebuild project symbol index after invalidation");

        assert_eq!(analyze_calls.load(Ordering::SeqCst), 2);
        fs::remove_dir_all(root).ok();
    }
}

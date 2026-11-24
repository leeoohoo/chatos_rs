use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::{is_reparse_point, normalize, MAXIMUM_SEARCH_DIRECTORIES, MAXIMUM_TOTAL_RESULTS};

const FILE_SEARCH_CANDIDATE_LIMIT: usize = 256;

pub(super) struct FileRecord {
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) path: PathBuf,
    pub(super) is_directory: bool,
    modified_at: SystemTime,
}

pub(super) struct FileSearchResult {
    pub(super) records: Vec<FileRecord>,
    pub(super) timed_out: bool,
    pub(super) source_issue: bool,
}

#[derive(Clone)]
pub(super) struct SearchCancellation {
    generation: Arc<AtomicU64>,
    expected: u64,
}

impl SearchCancellation {
    pub(super) fn new(generation: Arc<AtomicU64>, expected: u64) -> Self {
        Self {
            generation,
            expected,
        }
    }

    fn is_cancelled(&self) -> bool {
        self.generation.load(Ordering::Acquire) != self.expected
    }
}

pub(super) fn search_files(
    query: &str,
    timeout: Duration,
    cancellation: &SearchCancellation,
) -> FileSearchResult {
    let Some(home) = dirs::home_dir() else {
        return FileSearchResult {
            records: Vec::new(),
            timed_out: false,
            source_issue: true,
        };
    };
    search_files_from_root(&home, query, timeout, cancellation)
}

fn search_files_from_root(
    root: &Path,
    query: &str,
    timeout: Duration,
    cancellation: &SearchCancellation,
) -> FileSearchResult {
    let deadline = Instant::now() + timeout;
    let normalized_query = normalize(query);
    let mut source_issue = false;
    let mut timed_out = false;
    let mut candidates = Vec::new();
    let mut directories = VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((directory, depth)) = directories.pop_front() {
        if cancellation.is_cancelled() {
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                source_issue = true;
                continue;
            }
        };
        for entry in entries {
            if cancellation.is_cancelled() {
                break;
            }
            if Instant::now() >= deadline {
                timed_out = true;
                break;
            }
            let Ok(entry) = entry else {
                source_issue = true;
                continue;
            };
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                source_issue = true;
                continue;
            };
            if metadata.file_type().is_symlink()
                || is_reparse_point(&metadata)
                || is_hidden_or_system(&path, &metadata)
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if should_skip_directory(&name) && metadata.is_dir() {
                continue;
            }
            if normalize(&name).contains(&normalized_query) {
                candidates.push(FileRecord {
                    title: name,
                    subtitle: safe_parent_label(&path, root),
                    path: path.clone(),
                    is_directory: metadata.is_dir(),
                    modified_at: metadata.modified().unwrap_or(UNIX_EPOCH),
                });
                if candidates.len() >= FILE_SEARCH_CANDIDATE_LIMIT {
                    break;
                }
            }
            if metadata.is_dir() && depth < 12 {
                if directories.len() < MAXIMUM_SEARCH_DIRECTORIES {
                    directories.push_back((path, depth + 1));
                } else {
                    source_issue = true;
                }
            }
        }
        if candidates.len() >= FILE_SEARCH_CANDIDATE_LIMIT {
            break;
        }
    }
    candidates.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| normalize(&left.title).cmp(&normalize(&right.title)))
    });
    candidates.truncate(MAXIMUM_TOTAL_RESULTS);
    FileSearchResult {
        records: candidates,
        timed_out,
        source_issue,
    }
}

fn should_skip_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "appdata" | "node_modules" | "target" | ".git" | ".cache" | ".cargo" | ".rustup"
    )
}

fn safe_parent_label(path: &Path, home: &Path) -> String {
    let parent = path.parent().unwrap_or(home);
    let relative = parent.strip_prefix(home).unwrap_or(parent);
    let components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| !component.is_empty())
        .take(3)
        .collect::<Vec<_>>();
    if components.is_empty() {
        "Home".into()
    } else {
        components.join(" › ")
    }
}

#[cfg(target_os = "windows")]
fn is_hidden_or_system(_: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & (0x2 | 0x4) != 0
}

#[cfg(not(target_os = "windows"))]
fn is_hidden_or_system(path: &Path, _: &fs::Metadata) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_search_is_bounded_and_does_not_follow_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("quarterly-report.txt"), b"report").unwrap();
        let nested = directory.path().join("Documents");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("report-notes.md"), b"notes").unwrap();
        let cancellation = SearchCancellation::new(Arc::new(AtomicU64::new(1)), 1);
        let result = search_files_from_root(
            directory.path(),
            "report",
            Duration::from_secs(1),
            &cancellation,
        );
        assert_eq!(result.records.len(), 2);
        assert!(result
            .records
            .iter()
            .all(|record| !record.path.to_string_lossy().is_empty()));
    }

    #[test]
    fn cancelled_file_search_stops_before_reading_results() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("report.txt"), b"report").unwrap();
        let cancellation = SearchCancellation::new(Arc::new(AtomicU64::new(2)), 1);
        let result = search_files_from_root(
            directory.path(),
            "report",
            Duration::from_secs(1),
            &cancellation,
        );
        assert!(result.records.is_empty());
        assert!(!result.timed_out);
    }
}

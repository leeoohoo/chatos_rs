use regex::RegexBuilder;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const DEFAULT_MAX_RESULTS: usize = 100;
pub const MAX_RESULTS: usize = 500;
pub const DEFAULT_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const DEFAULT_MAX_VISITS: usize = 20_000;

const DEFAULT_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".gradle",
    ".next",
    ".nuxt",
];

#[derive(Clone, Debug)]
pub struct TextSearchRequest {
    pub root: PathBuf,
    pub query: String,
    pub max_results: usize,
    pub max_file_bytes: u64,
    pub max_visits: usize,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchMatch {
    pub path: String,
    pub relative_path: String,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TextSearchResponse {
    pub entries: Vec<SearchMatch>,
    pub truncated: bool,
    pub visited_dirs: usize,
}

pub fn search_text(request: &TextSearchRequest) -> Result<TextSearchResponse, String> {
    let root = request.root.clone();
    if !root.exists() {
        return Err("路径不存在".to_string());
    }
    if !root.is_dir() {
        return Err("路径不是目录".to_string());
    }

    let query = request.query.trim();
    if query.is_empty() {
        return Err("搜索关键字不能为空".to_string());
    }

    let limit = request.max_results.clamp(1, MAX_RESULTS);
    let max_visits = request.max_visits.max(1);
    let pattern = if request.whole_word {
        format!(r"\b{}\b", regex::escape(query))
    } else {
        regex::escape(query)
    };
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(!request.case_sensitive)
        .unicode(true)
        .build()
        .map_err(|err| err.to_string())?;

    let walker = WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| should_visit(entry.path(), entry.depth()));

    let mut entries = Vec::new();
    let mut visited_dirs = 0usize;
    let mut truncated = false;

    for item in walker {
        let entry = match item {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        if entry.file_type().is_dir() {
            if entry.depth() > 0 {
                visited_dirs += 1;
                if visited_dirs > max_visits {
                    truncated = true;
                    break;
                }
            }
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }
        if entries.len() >= limit {
            truncated = true;
            break;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() > request.max_file_bytes {
            continue;
        }

        let buffer = match fs::read(entry.path()) {
            Ok(buffer) => buffer,
            Err(_) => continue,
        };
        if !is_text_searchable(entry.path(), &buffer) {
            continue;
        }
        let content = match std::str::from_utf8(&buffer) {
            Ok(content) => content,
            Err(_) => continue,
        };

        for (index, line) in content.split('\n').enumerate() {
            let normalized = line.trim_end_matches('\r');
            let snippet = if normalized.len() > 400 {
                normalized[..400].to_string()
            } else {
                normalized.to_string()
            };
            let path = entry.path().to_string_lossy().to_string();
            let relative_path = pathdiff::diff_paths(entry.path(), &root)
                .unwrap_or_else(|| entry.path().to_path_buf())
                .to_string_lossy()
                .to_string();

            for found in regex.find_iter(normalized) {
                if entries.len() >= limit {
                    truncated = true;
                    break;
                }
                let column = normalized[..found.start()].chars().count() + 1;
                entries.push(SearchMatch {
                    path: path.clone(),
                    relative_path: relative_path.clone(),
                    line: index + 1,
                    column,
                    text: snippet.clone(),
                });
            }

            if truncated {
                break;
            }
        }
    }

    entries.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });

    Ok(TextSearchResponse {
        entries,
        truncated,
        visited_dirs,
    })
}

fn should_visit(path: &Path, depth: usize) -> bool {
    if depth == 0 {
        return true;
    }
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return true;
    };
    !DEFAULT_IGNORED_DIRS.contains(&name)
}

fn is_text_searchable(path: &Path, bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes
        .iter()
        .take(bytes.len().min(8192))
        .any(|byte| *byte == 0)
    {
        return false;
    }

    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    let text_like_extension = matches!(
        ext.as_str(),
        "rs" | "toml"
            | "lock"
            | "md"
            | "txt"
            | "json"
            | "yaml"
            | "yml"
            | "xml"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "less"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "swift"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "php"
            | "rb"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "bat"
            | "ini"
            | "conf"
            | "env"
            | "log"
            | "sql"
            | "vue"
            | "svelte"
            | "astro"
            | "dart"
            | "lua"
            | "r"
            | "m"
            | "mm"
            | "scala"
            | "gradle"
            | "make"
            | "cmake"
            | "properties"
            | "cfg"
            | "rc"
            | "proto"
            | "graphql"
    );

    if text_like_extension {
        return std::str::from_utf8(bytes).is_ok();
    }

    std::str::from_utf8(bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{search_text, TextSearchRequest, DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_VISITS};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_root() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("workspace_search_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn search_text_returns_line_and_relative_path() {
        let root = make_temp_root();
        fs::write(root.join("README.md"), "hello search world\nsecond line").expect("write file");

        let response = search_text(&TextSearchRequest {
            root: root.clone(),
            query: "search".to_string(),
            max_results: 50,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_visits: DEFAULT_MAX_VISITS,
            case_sensitive: true,
            whole_word: false,
        })
        .expect("search text");

        assert_eq!(response.entries.len(), 1);
        let hit = &response.entries[0];
        assert_eq!(hit.relative_path, "README.md");
        assert_eq!(hit.line, 1);
        assert_eq!(hit.column, 7);

        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn search_text_whole_word_avoids_partial_matches() {
        let root = make_temp_root();
        fs::write(
            root.join("sample.ts"),
            "const alias = 1;\nconst aliasName = 2;\n",
        )
        .expect("write file");

        let response = search_text(&TextSearchRequest {
            root: root.clone(),
            query: "alias".to_string(),
            max_results: 50,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_visits: DEFAULT_MAX_VISITS,
            case_sensitive: true,
            whole_word: true,
        })
        .expect("search text");

        assert_eq!(response.entries.len(), 1);
        assert_eq!(response.entries[0].line, 1);

        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn search_text_returns_multiple_matches_on_same_line() {
        let root = make_temp_root();
        fs::write(root.join("sample.ts"), "foo foo foo\n").expect("write file");

        let response = search_text(&TextSearchRequest {
            root: root.clone(),
            query: "foo".to_string(),
            max_results: 50,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_visits: DEFAULT_MAX_VISITS,
            case_sensitive: true,
            whole_word: false,
        })
        .expect("search text");

        assert_eq!(response.entries.len(), 3);
        assert_eq!(response.entries[0].column, 1);
        assert_eq!(response.entries[1].column, 5);
        assert_eq!(response.entries[2].column, 9);

        fs::remove_dir_all(root).expect("cleanup root");
    }
}

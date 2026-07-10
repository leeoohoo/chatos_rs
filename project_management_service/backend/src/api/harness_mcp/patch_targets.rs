// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::path_policy::optional_repo_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PatchTarget {
    pub(super) before_path: String,
    pub(super) after_path: String,
}

pub(super) fn collect_patch_targets(patch: &str) -> Result<Vec<PatchTarget>, String> {
    let text = patch.replace("\r\n", "\n");
    let lines = text.split('\n').collect::<Vec<_>>();
    let mut targets = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let before_path = optional_repo_path(Some(path), false)?;
            let mut after_path = before_path.clone();
            i += 1;
            while i < lines.len() {
                let current = lines[i];
                if is_patch_boundary(current) {
                    break;
                }
                if let Some(dest) = current.strip_prefix("*** Move to: ") {
                    after_path = optional_repo_path(Some(dest), false)?;
                }
                i += 1;
            }
            targets.push(PatchTarget {
                before_path,
                after_path,
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let path = optional_repo_path(Some(path), false)?;
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            i += 1;
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let path = optional_repo_path(Some(path), false)?;
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            i += 1;
            continue;
        }
        if let Some(path) = parse_loose_update_header(line) {
            let path = optional_repo_path(Some(path.as_str()), false)?;
            targets.push(PatchTarget {
                before_path: path.clone(),
                after_path: path,
            });
            i += 1;
            continue;
        }
        i += 1;
    }
    targets.sort_by(|left, right| {
        (&left.before_path, &left.after_path).cmp(&(&right.before_path, &right.after_path))
    });
    targets.dedup();
    Ok(targets)
}

fn parse_loose_update_header(line: &str) -> Option<String> {
    let trimmed = line.trim();
    for prefix in ["Update File --- ", "Update File: "] {
        let Some(path) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let path = path.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    None
}

fn is_patch_boundary(line: &str) -> bool {
    line.starts_with("*** Update File: ")
        || line.starts_with("*** Add File: ")
        || line.starts_with("*** Delete File: ")
        || line.starts_with("*** End Patch")
}

pub(super) fn patch_error_with_recovery(error: &str) -> String {
    if error.contains("Patch context not found in file.")
        || error.contains("old_text not found in file.")
    {
        let hint = json!({
            "error": error,
            "recovery": {
                "recommended_next_tools": [
                    "read_file_raw",
                    "read_file_range"
                ],
                "guidance": "Patch context is stale. Re-read target files from Harness and regenerate the patch with exact current lines."
            }
        });
        serde_json::to_string(&hint).unwrap_or_else(|_| error.to_string())
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_targets_include_multiple_sections_and_moves() {
        let patch = r#"*** Begin Patch
*** Update File: src/a.rs
@@
-old
+new
*** Update File: src/b.rs
*** Move to: src/c.rs
@@
-b
+c
*** Add File: src/d.rs
+hello
*** Delete File: src/e.rs
*** End Patch
"#;

        let targets = collect_patch_targets(patch).expect("targets");

        assert_eq!(
            targets,
            vec![
                PatchTarget {
                    before_path: "src/a.rs".to_string(),
                    after_path: "src/a.rs".to_string(),
                },
                PatchTarget {
                    before_path: "src/b.rs".to_string(),
                    after_path: "src/c.rs".to_string(),
                },
                PatchTarget {
                    before_path: "src/d.rs".to_string(),
                    after_path: "src/d.rs".to_string(),
                },
                PatchTarget {
                    before_path: "src/e.rs".to_string(),
                    after_path: "src/e.rs".to_string(),
                },
            ]
        );
    }
}

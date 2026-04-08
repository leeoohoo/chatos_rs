use std::path::{Path, PathBuf};

fn sanitize_segment(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }

    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    let compact = out.trim_matches('_').to_string();
    if compact.is_empty() {
        "unknown".to_string()
    } else {
        compact
    }
}

fn resolve_home_notepad_root() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let current = home.join(".agent_workspace").join("notepad");
    if current.exists() {
        return current;
    }
    let Ok(entries) = std::fs::read_dir(&home) else {
        return current;
    };
    for entry in entries.flatten() {
        let candidate = entry.path().join("notepad");
        if candidate.is_dir() {
            return candidate;
        }
    }
    current
}

fn resolve_notepad_root() -> PathBuf {
    if let Ok(raw) = std::env::var("NOTEPAD_DATA_DIR") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let container_data_dir = Path::new("/app/data");
    if container_data_dir.exists() {
        return container_data_dir.join("notepad");
    }

    resolve_home_notepad_root()
}

fn resolve_user_root(user_id: &str) -> PathBuf {
    let user_seg = sanitize_segment(user_id);
    resolve_notepad_root().join(user_seg)
}

pub fn resolve_data_dir(user_id: &str) -> PathBuf {
    resolve_user_root(user_id).join("__global__")
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<bool, String> {
    if !source.exists() {
        return Ok(false);
    }

    std::fs::create_dir_all(target).map_err(|err| err.to_string())?;
    let mut copied_any = false;
    for entry in std::fs::read_dir(source).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            copied_any |= copy_dir_recursive(source_path.as_path(), target_path.as_path())?;
            continue;
        }
        if !source_path.is_file() || target_path.exists() {
            continue;
        }

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        std::fs::copy(source_path.as_path(), target_path.as_path())
            .map_err(|err| err.to_string())?;
        copied_any = true;
    }

    Ok(copied_any)
}

pub fn migrate_legacy_project_data(user_id: &str, target_data_dir: &Path) -> Result<(), String> {
    let current_root = resolve_user_root(user_id);
    let home_root = resolve_home_notepad_root().join(sanitize_segment(user_id));
    let mut source_roots = vec![current_root];
    if source_roots[0] != home_root {
        source_roots.push(home_root);
    }

    let target_notes_root = target_data_dir.join("notes");
    std::fs::create_dir_all(&target_notes_root).map_err(|err| err.to_string())?;

    let mut copied_any = false;
    for source_root in source_roots {
        copied_any |= copy_dir_recursive(
            source_root.join("__global__").join("notes").as_path(),
            target_notes_root.as_path(),
        )?;

        let entries = match std::fs::read_dir(&source_root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.to_string()),
        };

        for entry in entries {
            let entry = entry.map_err(|err| err.to_string())?;
            let scope_path = entry.path();
            if !scope_path.is_dir() {
                continue;
            }

            let scope = entry.file_name().to_string_lossy().to_string();
            if scope == "__global__" {
                continue;
            }

            let legacy_notes_root = scope_path.join("notes");
            if !legacy_notes_root.is_dir() {
                continue;
            }

            let target_scope_root = target_notes_root.join(scope);
            copied_any |=
                copy_dir_recursive(legacy_notes_root.as_path(), target_scope_root.as_path())?;
        }
    }

    if copied_any {
        let index_path = target_data_dir.join("notes-index.json");
        if index_path.exists() {
            let _ = std::fs::remove_file(index_path);
        }
    }

    Ok(())
}

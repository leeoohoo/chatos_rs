use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio::fs;
use uuid::Uuid;
use walkdir::WalkDir;

use super::store_lock::acquire_file_lock;
use super::store_normalize::{
    extract_title_from_markdown, normalize_folder_path, normalize_string, normalize_title, now_iso,
    split_folder, ts_to_rfc3339, unique_tags,
};
use super::types::{NoteIndexEntry, NoteOutput, NotesIndex, INDEX_VERSION};

mod folder_ops;
mod note_ops;

fn entry_to_output(entry: &NoteIndexEntry) -> NoteOutput {
    NoteOutput::from_entry(entry)
}

fn normalize_index(mut index: NotesIndex) -> NotesIndex {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in index.notes.drain(..) {
        let id = normalize_string(&item.id);
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }

        let folder = normalize_folder_path(item.folder.as_str()).unwrap_or_default();
        let title = {
            let t = normalize_title(item.title.as_str());
            if t.is_empty() {
                "Untitled".to_string()
            } else {
                t
            }
        };
        let tags = unique_tags(&item.tags);
        let created_at = {
            let value = normalize_string(item.created_at.as_str());
            if value.is_empty() {
                now_iso()
            } else {
                value
            }
        };
        let updated_at = {
            let value = normalize_string(item.updated_at.as_str());
            if value.is_empty() {
                created_at.clone()
            } else {
                value
            }
        };

        out.push(NoteIndexEntry {
            id,
            title,
            folder,
            tags,
            created_at,
            updated_at,
        });
    }

    NotesIndex {
        version: INDEX_VERSION,
        notes: out,
    }
}

pub struct NotepadStore {
    data_dir: PathBuf,
    notes_root: PathBuf,
    index_path: PathBuf,
    lock_path: PathBuf,
}

impl NotepadStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let notes_root = data_dir.join("notes");
        let index_path = data_dir.join("notes-index.json");
        let lock_path = data_dir.join("notes.lock");
        Self {
            data_dir,
            notes_root,
            index_path,
            lock_path,
        }
    }

    async fn with_lock<F, Fut, T>(&self, action: F) -> Result<T, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let _guard = acquire_file_lock(self.lock_path.as_path()).await?;
        action().await
    }

    fn note_abs_path(&self, folder: &str, id: &str) -> PathBuf {
        let mut out = self.notes_root.clone();
        for segment in split_folder(folder) {
            out.push(segment);
        }
        out.push(format!("{id}.md"));
        out
    }

    async fn atomic_write_text(path: &Path, text: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|err| err.to_string())?;
        }

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("tmp");
        let tmp_name = format!(".{file_name}.{}.{}.tmp", std::process::id(), Uuid::new_v4());
        let tmp_path = path.with_file_name(tmp_name);

        fs::write(&tmp_path, text.as_bytes())
            .await
            .map_err(|err| err.to_string())?;

        match fs::rename(&tmp_path, path).await {
            Ok(_) => Ok(()),
            Err(err) => {
                let _ = fs::remove_file(path).await;
                fs::rename(&tmp_path, path)
                    .await
                    .map_err(|rename_err| format!("{}; {}", err, rename_err))
            }
        }
    }

    fn list_markdown_files(&self) -> Vec<PathBuf> {
        if !self.notes_root.exists() {
            return Vec::new();
        }

        WalkDir::new(&self.notes_root)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
            })
            .map(|entry| entry.path().to_path_buf())
            .collect()
    }

    fn rebuild_index_from_filesystem_sync(&self) -> NotesIndex {
        let mut notes = Vec::new();
        for file_abs in self.list_markdown_files() {
            let Some(stem) = file_abs.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let id = normalize_string(stem);
            if id.is_empty() {
                continue;
            }

            let relative = match file_abs.strip_prefix(&self.notes_root) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let folder = relative
                .parent()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .map(|value| value.trim_matches('/').to_string())
                .unwrap_or_default();

            let content = std::fs::read_to_string(&file_abs).unwrap_or_default();
            let title = {
                let from_content =
                    normalize_title(extract_title_from_markdown(content.as_str()).as_str());
                if from_content.is_empty() {
                    "Untitled".to_string()
                } else {
                    from_content
                }
            };

            let (created_at, updated_at) = match std::fs::metadata(&file_abs) {
                Ok(meta) => {
                    let created = meta
                        .created()
                        .ok()
                        .map(ts_to_rfc3339)
                        .unwrap_or_else(now_iso);
                    let updated = meta
                        .modified()
                        .ok()
                        .map(ts_to_rfc3339)
                        .unwrap_or_else(|| created.clone());
                    (created, updated)
                }
                Err(_) => {
                    let now = now_iso();
                    (now.clone(), now)
                }
            };

            notes.push(NoteIndexEntry {
                id,
                title,
                folder,
                tags: Vec::new(),
                created_at,
                updated_at,
            });
        }

        normalize_index(NotesIndex {
            version: INDEX_VERSION,
            notes,
        })
    }

    async fn rebuild_index_from_filesystem(&self) -> Result<NotesIndex, String> {
        let rebuilt = self.rebuild_index_from_filesystem_sync();
        Self::atomic_write_text(
            self.index_path.as_path(),
            serde_json::to_string_pretty(&rebuilt)
                .map_err(|err| err.to_string())?
                .as_str(),
        )
        .await?;
        Ok(rebuilt)
    }

    async fn load_index_locked(&self) -> Result<NotesIndex, String> {
        fs::create_dir_all(&self.notes_root)
            .await
            .map_err(|err| err.to_string())?;

        if fs::metadata(&self.index_path).await.is_err() {
            return self.rebuild_index_from_filesystem().await;
        }

        let raw = match fs::read_to_string(&self.index_path).await {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return self.rebuild_index_from_filesystem().await;
            }
            Err(err) => return Err(err.to_string()),
        };

        let parsed = match serde_json::from_str::<NotesIndex>(&raw) {
            Ok(value) => value,
            Err(_) => {
                let backup = self.data_dir.join(format!(
                    "notes-index.corrupted.{}.json",
                    Uuid::new_v4().simple()
                ));
                let _ = fs::copy(&self.index_path, &backup).await;
                return self.rebuild_index_from_filesystem().await;
            }
        };

        let normalized = normalize_index(parsed);
        if normalized.version != INDEX_VERSION {
            let mut to_save = normalized.clone();
            to_save.version = INDEX_VERSION;
            self.save_index_locked(&to_save).await?;
            return Ok(to_save);
        }

        Ok(normalized)
    }

    async fn save_index_locked(&self, index: &NotesIndex) -> Result<NotesIndex, String> {
        let mut normalized = normalize_index(index.clone());
        normalized.version = INDEX_VERSION;
        let text = serde_json::to_string_pretty(&normalized).map_err(|err| err.to_string())?;
        Self::atomic_write_text(self.index_path.as_path(), text.as_str()).await?;
        Ok(normalized)
    }

    async fn get_index_snapshot(&self) -> Result<NotesIndex, String> {
        self.with_lock(|| async { self.load_index_locked().await })
            .await
    }

    pub async fn init(&self) -> Result<Value, String> {
        let snapshot = self.get_index_snapshot().await?;
        Ok(json!({
            "ok": true,
            "data_dir": self.data_dir.to_string_lossy().to_string(),
            "notes_root": self.notes_root.to_string_lossy().to_string(),
            "index_path": self.index_path.to_string_lossy().to_string(),
            "version": INDEX_VERSION,
            "notes": snapshot.notes.len()
        }))
    }
}

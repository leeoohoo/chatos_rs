use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio::fs;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use uuid::Uuid;
use walkdir::WalkDir;

use super::types::{
    CreateNoteParams, ListNotesParams, NoteIndexEntry, NoteOutput, NotesIndex, SearchNotesParams,
    TagCount, UpdateNoteParams, INDEX_VERSION,
};

const LOCK_TIMEOUT_MS: u64 = 10_000;
const LOCK_STALE_MS: u64 = 30_000;
const LOCK_POLL_MS: u64 = 25;

fn normalize_string(value: &str) -> String {
    value.trim().to_string()
}

fn normalize_title(value: &str) -> String {
    let out = normalize_string(value);
    if out.is_empty() {
        String::new()
    } else {
        out.chars().take(120).collect()
    }
}

fn normalize_tag(value: &str) -> String {
    normalize_string(value)
}

fn unique_tags(tags: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for tag in tags {
        let normalized = normalize_tag(tag);
        if normalized.is_empty() {
            continue;
        }
        let key = normalized.to_lowercase();
        if seen.insert(key) {
            out.push(normalized);
        }
    }
    out
}

fn is_valid_path_segment(segment: &str) -> bool {
    let s = segment.trim();
    if s.is_empty() || s == "." || s == ".." {
        return false;
    }
    if s.chars().any(|ch| {
        matches!(
            ch,
            '<' | '>' | ':' | '\"' | '/' | '\\' | '|' | '?' | '*' | '\0'
        )
    }) {
        return false;
    }
    !s.chars().any(|ch| (ch as u32) < 32)
}

fn normalize_folder_path(value: &str) -> Result<String, String> {
    let raw = normalize_string(value).replace('\\', "/");
    if raw.is_empty() {
        return Ok(String::new());
    }

    let cleaned = raw.trim_matches('/').to_string();
    if cleaned.is_empty() {
        return Ok(String::new());
    }

    let mut out = Vec::new();
    for part in cleaned.split('/').filter(|item| !item.trim().is_empty()) {
        if !is_valid_path_segment(part) {
            return Err(format!("Invalid folder segment: {part}"));
        }
        out.push(part.trim().to_string());
    }

    Ok(out.join("/"))
}

fn split_folder(folder: &str) -> Vec<String> {
    folder
        .trim()
        .replace('\\', "/")
        .split('/')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.trim().to_string())
        .collect()
}

fn extract_title_from_markdown(markdown: &str) -> String {
    let normalized = markdown.replace("\r\n", "\n");
    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if !heading.is_empty() {
                return heading.chars().take(120).collect();
            }
        }
        return trimmed.chars().take(120).collect();
    }
    String::new()
}

fn now_iso() -> String {
    crate::core::time::now_rfc3339()
}

fn ts_to_rfc3339(ts: SystemTime) -> String {
    let datetime: DateTime<Utc> = DateTime::<Utc>::from(ts);
    datetime.to_rfc3339()
}

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

struct FileLockGuard {
    path: PathBuf,
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn acquire_file_lock(path: &Path) -> Result<FileLockGuard, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }

    let start = std::time::Instant::now();
    loop {
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .await
        {
            Ok(mut file) => {
                let payload = format!(
                    "{{\"pid\":{},\"started_at\":\"{}\"}}",
                    std::process::id(),
                    now_iso()
                );
                let _ = file.write_all(payload.as_bytes()).await;
                return Ok(FileLockGuard {
                    path: path.to_path_buf(),
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if let Ok(meta) = fs::metadata(path).await {
                    if let Ok(modified_at) = meta.modified() {
                        if let Ok(elapsed) = modified_at.elapsed() {
                            if elapsed > Duration::from_millis(LOCK_STALE_MS) {
                                let _ = fs::remove_file(path).await;
                                continue;
                            }
                        }
                    }
                }
                if start.elapsed() > Duration::from_millis(LOCK_TIMEOUT_MS) {
                    return Err(format!(
                        "Timed out waiting for lock ({})",
                        path.file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or("notes.lock")
                    ));
                }
                sleep(Duration::from_millis(LOCK_POLL_MS)).await;
            }
            Err(err) => return Err(err.to_string()),
        }
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

    pub async fn list_folders(&self) -> Result<Value, String> {
        fs::create_dir_all(&self.notes_root)
            .await
            .map_err(|err| err.to_string())?;

        let mut folders = vec![String::new()];
        for entry in WalkDir::new(&self.notes_root)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_dir())
        {
            let path = entry.path();
            if path == self.notes_root {
                continue;
            }
            let Ok(rel) = path.strip_prefix(&self.notes_root) else {
                continue;
            };
            let rel_norm = rel.to_string_lossy().replace('\\', "/");
            let rel_trimmed = rel_norm.trim_matches('/').to_string();
            if !rel_trimmed.is_empty() {
                folders.push(rel_trimmed);
            }
        }

        folders.sort();
        folders.dedup();

        Ok(json!({
            "ok": true,
            "folders": folders
        }))
    }

    pub async fn create_folder(&self, folder: &str) -> Result<Value, String> {
        let normalized = normalize_folder_path(folder)?;
        if normalized.is_empty() {
            return Err("folder is required".to_string());
        }

        let abs =
            split_folder(&normalized)
                .into_iter()
                .fold(self.notes_root.clone(), |mut acc, part| {
                    acc.push(part);
                    acc
                });

        fs::create_dir_all(abs)
            .await
            .map_err(|err| err.to_string())?;
        Ok(json!({
            "ok": true,
            "folder": normalized
        }))
    }

    pub async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String> {
        let from_rel = normalize_folder_path(from)?;
        let to_rel = normalize_folder_path(to)?;
        if from_rel.is_empty() {
            return Err("from is required".to_string());
        }
        if to_rel.is_empty() {
            return Err("to is required".to_string());
        }

        self.with_lock(|| async move {
            if from_rel == to_rel {
                return Ok(json!({
                    "ok": true,
                    "from": from_rel,
                    "to": to_rel,
                    "moved_notes": 0
                }));
            }

            let from_abs = split_folder(&from_rel).into_iter().fold(
                self.notes_root.clone(),
                |mut acc, part| {
                    acc.push(part);
                    acc
                },
            );
            let to_abs =
                split_folder(&to_rel)
                    .into_iter()
                    .fold(self.notes_root.clone(), |mut acc, part| {
                        acc.push(part);
                        acc
                    });

            let from_meta = fs::metadata(&from_abs)
                .await
                .map_err(|_| format!("Folder not found: {from_rel}"))?;
            if !from_meta.is_dir() {
                return Err(format!("Folder not found: {from_rel}"));
            }
            if fs::metadata(&to_abs).await.is_ok() {
                return Err(format!("Target folder already exists: {to_rel}"));
            }

            if let Some(parent) = to_abs.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|err| err.to_string())?;
            }
            fs::rename(&from_abs, &to_abs)
                .await
                .map_err(|err| err.to_string())?;

            let mut index = self.load_index_locked().await?;
            let mut moved_notes = 0usize;
            let now = now_iso();
            for note in &mut index.notes {
                let folder = note.folder.trim().replace('\\', "/");
                if folder == from_rel {
                    note.folder = to_rel.clone();
                    note.updated_at = now.clone();
                    moved_notes += 1;
                    continue;
                }
                let prefix = format!("{from_rel}/");
                if folder.starts_with(prefix.as_str()) {
                    let suffix = folder.strip_prefix(prefix.as_str()).unwrap_or_default();
                    note.folder = format!("{to_rel}/{suffix}");
                    note.updated_at = now.clone();
                    moved_notes += 1;
                }
            }
            self.save_index_locked(&index).await?;

            Ok(json!({
                "ok": true,
                "from": from_rel,
                "to": to_rel,
                "moved_notes": moved_notes
            }))
        })
        .await
    }

    pub async fn delete_folder(&self, folder: &str, recursive: bool) -> Result<Value, String> {
        let rel = normalize_folder_path(folder)?;
        if rel.is_empty() {
            return Err("folder is required".to_string());
        }

        self.with_lock(|| async move {
            let abs =
                split_folder(&rel)
                    .into_iter()
                    .fold(self.notes_root.clone(), |mut acc, part| {
                        acc.push(part);
                        acc
                    });
            let meta = fs::metadata(&abs)
                .await
                .map_err(|_| format!("Folder not found: {rel}"))?;
            if !meta.is_dir() {
                return Err(format!("Folder not found: {rel}"));
            }

            let mut index = self.load_index_locked().await?;
            let affected_ids: Vec<String> = index
                .notes
                .iter()
                .filter(|note| {
                    note.folder == rel || note.folder.starts_with(format!("{rel}/").as_str())
                })
                .map(|note| note.id.clone())
                .collect();

            if !recursive {
                fs::remove_dir(&abs).await.map_err(|err| err.to_string())?;
                return Ok(json!({
                    "ok": true,
                    "folder": rel,
                    "deleted_notes": 0
                }));
            }

            fs::remove_dir_all(&abs)
                .await
                .map_err(|err| err.to_string())?;

            let remove_set: HashSet<String> = affected_ids.iter().cloned().collect();
            index.notes.retain(|note| !remove_set.contains(&note.id));
            self.save_index_locked(&index).await?;

            Ok(json!({
                "ok": true,
                "folder": rel,
                "deleted_notes": affected_ids.len()
            }))
        })
        .await
    }

    pub async fn list_notes(&self, params: ListNotesParams) -> Result<Value, String> {
        let folder_rel = normalize_folder_path(params.folder.as_str())?;
        let desired_tags = unique_tags(&params.tags);
        let query = normalize_string(params.query.as_str()).to_lowercase();
        let limit = params.limit.clamp(1, 500);

        let snapshot = self.get_index_snapshot().await?;
        let mut notes = snapshot.notes;

        if !folder_rel.is_empty() {
            let prefix = format!("{folder_rel}/");
            notes.retain(|note| {
                if note.folder == folder_rel {
                    return true;
                }
                if !params.recursive {
                    return false;
                }
                note.folder.starts_with(prefix.as_str())
            });
        }

        if !desired_tags.is_empty() {
            let desired_keys: Vec<String> =
                desired_tags.iter().map(|tag| tag.to_lowercase()).collect();
            notes.retain(|note| {
                let note_tags: HashSet<String> =
                    note.tags.iter().map(|tag| tag.to_lowercase()).collect();
                if params.match_any {
                    desired_keys
                        .iter()
                        .any(|tag| note_tags.contains(tag.as_str()))
                } else {
                    desired_keys
                        .iter()
                        .all(|tag| note_tags.contains(tag.as_str()))
                }
            });
        }

        if !query.is_empty() {
            notes.retain(|note| {
                note.title.to_lowercase().contains(query.as_str())
                    || note.folder.to_lowercase().contains(query.as_str())
            });
        }

        notes.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let out: Vec<NoteOutput> = notes.iter().take(limit).map(entry_to_output).collect();

        Ok(json!({
            "ok": true,
            "notes": out
        }))
    }

    pub async fn create_note(&self, params: CreateNoteParams) -> Result<Value, String> {
        let folder_rel = normalize_folder_path(params.folder.as_str())?;
        let tags = unique_tags(&params.tags);

        self.with_lock(|| async move {
            let title_from_input = normalize_title(params.title.as_str());
            let title_from_content =
                normalize_title(extract_title_from_markdown(params.content.as_str()).as_str());
            let title = if !title_from_input.is_empty() {
                title_from_input
            } else if !title_from_content.is_empty() {
                title_from_content
            } else {
                "Untitled".to_string()
            };

            let content = {
                let trimmed = normalize_string(params.content.as_str());
                if trimmed.is_empty() {
                    format!("# {title}\n\n")
                } else {
                    params.content
                }
            };

            let id = Uuid::new_v4().to_string();
            let abs = self.note_abs_path(folder_rel.as_str(), id.as_str());
            if let Some(parent) = abs.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|err| err.to_string())?;
            }
            Self::atomic_write_text(abs.as_path(), content.as_str()).await?;

            let mut index = self.load_index_locked().await?;
            let now = now_iso();
            let note = NoteIndexEntry {
                id,
                title,
                folder: folder_rel,
                tags,
                created_at: now.clone(),
                updated_at: now,
            };
            index.notes.insert(0, note.clone());
            self.save_index_locked(&index).await?;

            Ok(json!({
                "ok": true,
                "note": entry_to_output(&note)
            }))
        })
        .await
    }

    pub async fn get_note(&self, id: &str) -> Result<Value, String> {
        let note_id = normalize_string(id);
        if note_id.is_empty() {
            return Err("id is required".to_string());
        }

        let snapshot = self.get_index_snapshot().await?;
        let note = snapshot
            .notes
            .iter()
            .find(|note| note.id == note_id)
            .cloned()
            .ok_or_else(|| format!("Note not found: {note_id}"))?;

        let abs = self.note_abs_path(note.folder.as_str(), note_id.as_str());
        let content = fs::read_to_string(abs)
            .await
            .map_err(|err| err.to_string())?;

        Ok(json!({
            "ok": true,
            "note": entry_to_output(&note),
            "content": content
        }))
    }

    pub async fn update_note(&self, params: UpdateNoteParams) -> Result<Value, String> {
        let note_id = normalize_string(params.id.as_str());
        if note_id.is_empty() {
            return Err("id is required".to_string());
        }

        self.with_lock(|| async move {
            let mut index = self.load_index_locked().await?;
            let Some(position) = index.notes.iter().position(|note| note.id == note_id) else {
                return Err(format!("Note not found: {note_id}"));
            };

            let mut note = index.notes[position].clone();
            let mut next_folder = note.folder.clone();
            if let Some(folder) = params.folder.as_ref() {
                next_folder = normalize_folder_path(folder.as_str())?;
            }

            let next_title = if let Some(title) = params.title.as_ref() {
                let normalized = normalize_title(title.as_str());
                if normalized.is_empty() {
                    note.title.clone()
                } else {
                    normalized
                }
            } else {
                note.title.clone()
            };

            let next_tags = if let Some(tags) = params.tags.as_ref() {
                unique_tags(tags)
            } else {
                note.tags.clone()
            };

            let old_abs = self.note_abs_path(note.folder.as_str(), note.id.as_str());
            let new_abs = self.note_abs_path(next_folder.as_str(), note.id.as_str());
            if old_abs != new_abs {
                if let Some(parent) = new_abs.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|err| err.to_string())?;
                }
                fs::rename(&old_abs, &new_abs)
                    .await
                    .map_err(|err| err.to_string())?;
            }

            if let Some(content) = params.content.as_ref() {
                Self::atomic_write_text(new_abs.as_path(), content.as_str()).await?;
            }

            note.folder = next_folder;
            note.title = next_title;
            note.tags = next_tags;
            note.updated_at = now_iso();

            index.notes[position] = note.clone();
            self.save_index_locked(&index).await?;

            Ok(json!({
                "ok": true,
                "note": entry_to_output(&note)
            }))
        })
        .await
    }

    pub async fn delete_note(&self, id: &str) -> Result<Value, String> {
        let note_id = normalize_string(id);
        if note_id.is_empty() {
            return Err("id is required".to_string());
        }

        self.with_lock(|| async move {
            let mut index = self.load_index_locked().await?;
            let Some(position) = index.notes.iter().position(|note| note.id == note_id) else {
                return Err(format!("Note not found: {note_id}"));
            };
            let note = index.notes.remove(position);

            let abs = self.note_abs_path(note.folder.as_str(), note.id.as_str());
            let _ = fs::remove_file(abs).await;

            self.save_index_locked(&index).await?;
            Ok(json!({
                "ok": true,
                "id": note_id
            }))
        })
        .await
    }

    pub async fn list_tags(&self) -> Result<Value, String> {
        let snapshot = self.get_index_snapshot().await?;
        let mut counts: HashMap<String, TagCount> = HashMap::new();

        for note in snapshot.notes {
            for tag in note.tags {
                let normalized = normalize_tag(tag.as_str());
                if normalized.is_empty() {
                    continue;
                }
                let key = normalized.to_lowercase();
                counts
                    .entry(key)
                    .and_modify(|item| item.count += 1)
                    .or_insert(TagCount {
                        tag: normalized,
                        count: 1,
                    });
            }
        }

        let mut tags: Vec<TagCount> = counts.into_values().collect();
        tags.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then_with(|| left.tag.to_lowercase().cmp(&right.tag.to_lowercase()))
        });

        Ok(json!({
            "ok": true,
            "tags": tags
        }))
    }

    pub async fn search_notes(&self, params: SearchNotesParams) -> Result<Value, String> {
        let query = normalize_string(params.query.as_str());
        if query.is_empty() {
            return Err("query is required".to_string());
        }

        let base = self
            .list_notes(ListNotesParams {
                folder: params.folder.clone(),
                recursive: params.recursive,
                tags: params.tags.clone(),
                match_any: params.match_any,
                query: String::new(),
                limit: 500,
            })
            .await?;

        let candidates = base
            .get("notes")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        let mut results = Vec::new();
        let lower = query.to_lowercase();
        for note in candidates {
            if results.len() >= params.limit.clamp(1, 200) {
                break;
            }

            let title = note
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let folder = note
                .get("folder")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let id = note
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or("");

            if title.to_lowercase().contains(lower.as_str())
                || folder.to_lowercase().contains(lower.as_str())
            {
                results.push(note);
                continue;
            }

            if !params.include_content || id.trim().is_empty() {
                continue;
            }

            let abs = self.note_abs_path(folder, id);
            if let Ok(content) = fs::read_to_string(abs).await {
                if content.to_lowercase().contains(lower.as_str()) {
                    results.push(note);
                }
            }
        }

        Ok(json!({
            "ok": true,
            "notes": results
        }))
    }
}

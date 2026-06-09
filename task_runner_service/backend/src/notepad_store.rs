use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chatos_builtin_tools::NotepadStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::fs;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NoteMeta {
    id: String,
    title: String,
    folder: String,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotesIndex {
    version: i64,
    notes: Vec<NoteMeta>,
}

impl Default for NotesIndex {
    fn default() -> Self {
        Self {
            version: 1,
            notes: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct TaskRunnerNotepadStore {
    data_dir: PathBuf,
    notes_root: PathBuf,
    index_path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl TaskRunnerNotepadStore {
    pub fn new(root: PathBuf, user_id: &str) -> Result<Self, String> {
        let normalized_user = normalize_user_segment(user_id);
        let data_dir = root.join(normalized_user);
        let notes_root = data_dir.join("notes");
        let index_path = data_dir.join("notes-index.json");
        std::fs::create_dir_all(&notes_root).map_err(|err| err.to_string())?;
        if !index_path.exists() {
            std::fs::write(
                &index_path,
                serde_json::to_vec_pretty(&NotesIndex::default()).map_err(|err| err.to_string())?,
            )
            .map_err(|err| err.to_string())?;
        }
        Ok(Self {
            data_dir,
            notes_root,
            index_path,
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    async fn ensure_initialized(&self) -> Result<(), String> {
        fs::create_dir_all(&self.notes_root)
            .await
            .map_err(|err| err.to_string())?;
        if fs::metadata(&self.index_path).await.is_err() {
            let text = serde_json::to_string_pretty(&NotesIndex::default())
                .map_err(|err| err.to_string())?;
            write_atomic(&self.index_path, text.as_bytes()).await?;
        }
        Ok(())
    }

    async fn load_index(&self) -> Result<NotesIndex, String> {
        self.ensure_initialized().await?;
        let bytes = fs::read(&self.index_path)
            .await
            .map_err(|err| err.to_string())?;
        if bytes.is_empty() {
            return Ok(NotesIndex::default());
        }
        serde_json::from_slice(&bytes).map_err(|err| err.to_string())
    }

    async fn save_index(&self, index: &NotesIndex) -> Result<(), String> {
        self.ensure_initialized().await?;
        let text = serde_json::to_string_pretty(index).map_err(|err| err.to_string())?;
        write_atomic(&self.index_path, text.as_bytes()).await
    }

    fn note_path(&self, folder: &str, id: &str) -> PathBuf {
        let mut path = self.notes_root.clone();
        for segment in folder_segments(folder) {
            path.push(segment);
        }
        path.push(format!("{id}.md"));
        path
    }

    fn note_output(&self, note: &NoteMeta) -> Value {
        let folder = note.folder.clone();
        let file = if folder.is_empty() {
            format!("notes/{}.md", note.id)
        } else {
            format!("notes/{}/{}.md", folder, note.id)
        };
        json!({
            "id": note.id,
            "title": note.title,
            "folder": folder,
            "tags": note.tags,
            "created_at": note.created_at,
            "updated_at": note.updated_at,
            "file": file,
        })
    }

    async fn find_note(&self, id: &str) -> Result<NoteMeta, String> {
        let note_id = normalize_required(id, "id")?;
        let index = self.load_index().await?;
        index
            .notes
            .into_iter()
            .find(|note| note.id == note_id)
            .ok_or_else(|| format!("Note not found: {note_id}"))
    }
}

#[async_trait]
impl NotepadStore for TaskRunnerNotepadStore {
    async fn init(&self) -> Result<Value, String> {
        self.ensure_initialized().await?;
        Ok(json!({
            "ok": true,
            "data_dir": self.data_dir.to_string_lossy(),
            "notes_root": self.notes_root.to_string_lossy(),
        }))
    }

    async fn list_folders(&self) -> Result<Value, String> {
        let index = self.load_index().await?;
        let mut folders = BTreeSet::new();
        for note in index.notes {
            let mut current = String::new();
            for segment in folder_segments(note.folder.as_str()) {
                if current.is_empty() {
                    current = segment.to_string();
                } else {
                    current = format!("{current}/{segment}");
                }
                folders.insert(current.clone());
            }
        }
        Ok(json!({
            "ok": true,
            "folders": folders.into_iter().collect::<Vec<_>>(),
        }))
    }

    async fn create_folder(&self, folder: &str) -> Result<Value, String> {
        let folder = normalize_folder(folder)?;
        let path = self.notes_root.join(folder.as_str());
        fs::create_dir_all(path)
            .await
            .map_err(|err| err.to_string())?;
        Ok(json!({
            "ok": true,
            "folder": folder,
        }))
    }

    async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String> {
        let from = normalize_folder(from)?;
        let to = normalize_folder(to)?;
        if from == to {
            return Ok(json!({ "ok": true, "from": from, "to": to }));
        }

        let _guard = self.write_lock.lock().await;
        let mut index = self.load_index().await?;
        let from_prefix = format!("{from}/");
        let affected = index
            .notes
            .iter()
            .filter(|note| note.folder == from || note.folder.starts_with(from_prefix.as_str()))
            .count();
        if affected == 0 {
            return Err(format!("Folder not found: {from}"));
        }

        let old_path = self.notes_root.join(from.as_str());
        let new_path = self.notes_root.join(to.as_str());
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|err| err.to_string())?;
        }
        fs::rename(&old_path, &new_path)
            .await
            .map_err(|err| err.to_string())?;

        for note in &mut index.notes {
            if note.folder == from {
                note.folder = to.clone();
            } else if note.folder.starts_with(from_prefix.as_str()) {
                note.folder = format!("{}{}", to, &note.folder[from.len()..]);
            }
            note.updated_at = now_iso();
        }
        self.save_index(&index).await?;
        Ok(json!({
            "ok": true,
            "from": from,
            "to": to,
            "updated_notes": affected,
        }))
    }

    async fn delete_folder(&self, folder: &str, recursive: bool) -> Result<Value, String> {
        let folder = normalize_folder(folder)?;
        let _guard = self.write_lock.lock().await;
        let mut index = self.load_index().await?;
        let prefix = format!("{folder}/");
        let matching_ids = index
            .notes
            .iter()
            .filter(|note| note.folder == folder || note.folder.starts_with(prefix.as_str()))
            .map(|note| note.id.clone())
            .collect::<Vec<_>>();
        if !recursive && !matching_ids.is_empty() {
            return Err("folder is not empty; pass recursive=true to delete".to_string());
        }
        if recursive {
            let removed = index
                .notes
                .iter()
                .filter(|note| note.folder == folder || note.folder.starts_with(prefix.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            index
                .notes
                .retain(|note| note.folder != folder && !note.folder.starts_with(prefix.as_str()));
            for note in removed {
                let path = self.note_path(note.folder.as_str(), note.id.as_str());
                let _ = fs::remove_file(path).await;
            }
            self.save_index(&index).await?;
        }
        let dir = self.notes_root.join(folder.as_str());
        if recursive {
            let _ = fs::remove_dir_all(&dir).await;
        } else {
            fs::remove_dir(&dir).await.map_err(|err| err.to_string())?;
        }
        Ok(json!({
            "ok": true,
            "folder": folder,
            "deleted_notes": matching_ids.len(),
        }))
    }

    async fn list_notes(&self, params: Value) -> Result<Value, String> {
        let folder = normalize_optional_folder(value_string(&params, "folder"))?;
        let recursive = params
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let tags = normalize_tags(value_string_array(&params, "tags"));
        let match_any = params
            .get("match_any")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let query = value_string(&params, "query").to_ascii_lowercase();
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(200)
            .clamp(1, 500);

        let mut notes = self.load_index().await?.notes;
        filter_notes(
            &mut notes,
            folder.as_deref(),
            recursive,
            &tags,
            match_any,
            query.as_str(),
        );
        notes.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(json!({
            "ok": true,
            "notes": notes.iter().take(limit).map(|note| self.note_output(note)).collect::<Vec<_>>(),
        }))
    }

    async fn create_note(&self, params: Value) -> Result<Value, String> {
        let folder =
            normalize_optional_folder(value_string(&params, "folder"))?.unwrap_or_default();
        let requested_title = value_string(&params, "title");
        let content = value_string(&params, "content");
        let tags = normalize_tags(value_string_array(&params, "tags"));

        let _guard = self.write_lock.lock().await;
        let mut index = self.load_index().await?;
        let now = now_iso();
        let title = derive_title(requested_title.as_str(), content.as_str());
        let stored_content = if content.trim().is_empty() {
            format!("# {title}\n\n")
        } else {
            content
        };
        let id = Uuid::new_v4().to_string();
        let note = NoteMeta {
            id: id.clone(),
            title,
            folder,
            tags,
            created_at: now.clone(),
            updated_at: now,
        };
        let note_path = self.note_path(note.folder.as_str(), id.as_str());
        write_atomic(&note_path, stored_content.as_bytes()).await?;
        index.notes.insert(0, note.clone());
        self.save_index(&index).await?;
        Ok(json!({
            "ok": true,
            "note": self.note_output(&note),
        }))
    }

    async fn read_note(&self, id: &str) -> Result<Value, String> {
        let note = self.find_note(id).await?;
        let content = fs::read_to_string(self.note_path(note.folder.as_str(), note.id.as_str()))
            .await
            .map_err(|err| err.to_string())?;
        Ok(json!({
            "ok": true,
            "note": self.note_output(&note),
            "content": content,
        }))
    }

    async fn update_note(&self, params: Value) -> Result<Value, String> {
        let note_id = normalize_required(value_string(&params, "id").as_str(), "id")?;
        let title_patch = optional_non_empty(value_string(&params, "title"));
        let content_patch = params
            .get("content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let folder_patch = if params.get("folder").is_some() {
            Some(normalize_optional_folder(value_string(&params, "folder"))?.unwrap_or_default())
        } else {
            None
        };
        let tags_patch = if params.get("tags").is_some() {
            Some(normalize_tags(value_string_array(&params, "tags")))
        } else {
            None
        };

        let _guard = self.write_lock.lock().await;
        let mut index = self.load_index().await?;
        let position = index
            .notes
            .iter()
            .position(|note| note.id == note_id)
            .ok_or_else(|| format!("Note not found: {note_id}"))?;
        let mut note = index.notes[position].clone();
        let old_path = self.note_path(note.folder.as_str(), note.id.as_str());

        if let Some(folder) = folder_patch {
            note.folder = folder;
        }
        if let Some(title) = title_patch {
            note.title = title;
        }
        if let Some(tags) = tags_patch {
            note.tags = tags;
        }

        let new_path = self.note_path(note.folder.as_str(), note.id.as_str());
        if old_path != new_path {
            if let Some(parent) = new_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|err| err.to_string())?;
            }
            fs::rename(&old_path, &new_path)
                .await
                .map_err(|err| err.to_string())?;
        }
        if let Some(content) = content_patch {
            write_atomic(&new_path, content.as_bytes()).await?;
            if value_string(&params, "title").trim().is_empty() {
                note.title = derive_title(note.title.as_str(), content.as_str());
            }
        }

        note.updated_at = now_iso();
        index.notes[position] = note.clone();
        self.save_index(&index).await?;
        Ok(json!({
            "ok": true,
            "note": self.note_output(&note),
        }))
    }

    async fn delete_note(&self, id: &str) -> Result<Value, String> {
        let note_id = normalize_required(id, "id")?;
        let _guard = self.write_lock.lock().await;
        let mut index = self.load_index().await?;
        let position = index
            .notes
            .iter()
            .position(|note| note.id == note_id)
            .ok_or_else(|| format!("Note not found: {note_id}"))?;
        let note = index.notes.remove(position);
        let _ = fs::remove_file(self.note_path(note.folder.as_str(), note.id.as_str())).await;
        self.save_index(&index).await?;
        Ok(json!({
            "ok": true,
            "id": note_id,
        }))
    }

    async fn list_tags(&self) -> Result<Value, String> {
        let notes = self.load_index().await?.notes;
        let mut counts = BTreeMap::<String, usize>::new();
        for note in notes {
            for tag in note.tags {
                *counts.entry(tag).or_default() += 1;
            }
        }
        Ok(json!({
            "ok": true,
            "tags": counts
                .into_iter()
                .map(|(tag, count)| json!({ "tag": tag, "count": count }))
                .collect::<Vec<_>>(),
        }))
    }

    async fn search_notes(&self, params: Value) -> Result<Value, String> {
        let query = normalize_required(value_string(&params, "query").as_str(), "query")?;
        let folder = normalize_optional_folder(value_string(&params, "folder"))?;
        let recursive = params
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let tags = normalize_tags(value_string_array(&params, "tags"));
        let match_any = params
            .get("match_any")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let include_content = params
            .get("include_content")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(50)
            .clamp(1, 200);
        let mut notes = self.load_index().await?.notes;
        filter_notes(
            &mut notes,
            folder.as_deref(),
            recursive,
            &tags,
            match_any,
            "",
        );
        let needle = query.to_ascii_lowercase();
        let mut matches = Vec::new();
        for note in notes {
            let mut content_match = false;
            let mut preview = None;
            if include_content {
                if let Ok(content) =
                    fs::read_to_string(self.note_path(note.folder.as_str(), note.id.as_str())).await
                {
                    let lowered = content.to_ascii_lowercase();
                    if lowered.contains(needle.as_str()) {
                        content_match = true;
                        preview = Some(content.chars().take(240).collect::<String>());
                    }
                }
            }
            let title_match = note.title.to_ascii_lowercase().contains(needle.as_str());
            let folder_match = note.folder.to_ascii_lowercase().contains(needle.as_str());
            if title_match || folder_match || content_match {
                matches.push(json!({
                    "note": self.note_output(&note),
                    "match": {
                        "title": title_match,
                        "folder": folder_match,
                        "content": content_match,
                    },
                    "preview": preview,
                }));
            }
            if matches.len() >= limit {
                break;
            }
        }
        Ok(json!({
            "ok": true,
            "query": query,
            "results": matches,
        }))
    }
}

fn normalize_user_segment(user_id: &str) -> String {
    let raw = user_id.trim();
    if raw.is_empty() {
        return "task_runner".to_string();
    }
    let cleaned = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if cleaned.is_empty() {
        "task_runner".to_string()
    } else {
        cleaned
    }
}

fn folder_segments(folder: &str) -> Vec<&str> {
    folder
        .split('/')
        .filter(|segment| !segment.trim().is_empty())
        .collect()
}

fn normalize_folder(folder: &str) -> Result<String, String> {
    let raw = folder.trim().replace('\\', "/");
    if raw.is_empty() {
        return Err("folder is required".to_string());
    }
    let mut out = Vec::new();
    for segment in raw.split('/') {
        let normalized = segment.trim();
        if normalized.is_empty() || normalized == "." || normalized == ".." {
            return Err("folder contains invalid path segments".to_string());
        }
        out.push(normalized.to_string());
    }
    Ok(out.join("/"))
}

fn normalize_optional_folder(folder: String) -> Result<Option<String>, String> {
    if folder.trim().is_empty() {
        Ok(None)
    } else {
        normalize_folder(folder.as_str()).map(Some)
    }
}

fn normalize_required(value: &str, label: &str) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(normalized.to_string())
    }
}

fn value_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn value_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn optional_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .filter(|tag| seen.insert(tag.to_ascii_lowercase()))
        .collect()
}

fn derive_title(requested_title: &str, content: &str) -> String {
    if let Some(title) = optional_non_empty(requested_title.to_string()) {
        return title;
    }
    for line in content.lines() {
        let trimmed = line.trim();
        let trimmed = trimmed.strip_prefix('#').unwrap_or(trimmed).trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "Untitled".to_string()
}

fn filter_notes(
    notes: &mut Vec<NoteMeta>,
    folder: Option<&str>,
    recursive: bool,
    tags: &[String],
    match_any: bool,
    query: &str,
) {
    if let Some(folder) = folder {
        let prefix = format!("{folder}/");
        notes.retain(|note| {
            note.folder == folder || (recursive && note.folder.starts_with(prefix.as_str()))
        });
    }
    if !tags.is_empty() {
        let normalized_tags = tags
            .iter()
            .map(|tag| tag.to_ascii_lowercase())
            .collect::<Vec<_>>();
        notes.retain(|note| {
            let note_tags = note
                .tags
                .iter()
                .map(|tag| tag.to_ascii_lowercase())
                .collect::<BTreeSet<_>>();
            if match_any {
                normalized_tags
                    .iter()
                    .any(|tag| note_tags.contains(tag.as_str()))
            } else {
                normalized_tags
                    .iter()
                    .all(|tag| note_tags.contains(tag.as_str()))
            }
        });
    }
    if !query.is_empty() {
        let needle = query.to_ascii_lowercase();
        notes.retain(|note| {
            note.title.to_ascii_lowercase().contains(needle.as_str())
                || note.folder.to_ascii_lowercase().contains(needle.as_str())
        });
    }
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    fs::write(&tmp, bytes)
        .await
        .map_err(|err| err.to_string())?;
    fs::rename(&tmp, path).await.map_err(|err| err.to_string())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

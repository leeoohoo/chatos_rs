use std::collections::HashSet;

use serde_json::{json, Value};
use tokio::fs;
use uuid::Uuid;

use super::super::store_normalize::{
    extract_title_from_markdown, normalize_folder_path, normalize_string, normalize_title, now_iso,
    unique_tags,
};
use super::super::types::{CreateNoteParams, ListNotesParams, SearchNotesParams, UpdateNoteParams};
use super::{entry_to_output, NotepadStore};

impl NotepadStore {
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
        let out: Vec<_> = notes.iter().take(limit).map(entry_to_output).collect();

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
            let note = super::super::types::NoteIndexEntry {
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

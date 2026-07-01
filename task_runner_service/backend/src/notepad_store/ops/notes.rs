// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskRunnerNotepadStore {
    pub(super) async fn list_notes_value(&self, params: Value) -> Result<Value, String> {
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
            "notes": notes
                .iter()
                .take(limit)
                .map(|note| self.note_output(note))
                .collect::<Vec<_>>(),
        }))
    }

    pub(super) async fn create_note_value(&self, params: Value) -> Result<Value, String> {
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
        write_atomic_limited(
            &note_path,
            stored_content.as_bytes(),
            MAX_NOTE_CONTENT_BYTES,
        )
        .await?;
        index.notes.insert(0, note.clone());
        self.save_index(&index).await?;
        Ok(json!({
            "ok": true,
            "note": self.note_output(&note),
        }))
    }

    pub(super) async fn read_note_value(&self, id: &str) -> Result<Value, String> {
        let note = self.find_note(id).await?;
        let note_path = self.note_path(note.folder.as_str(), note.id.as_str());
        let content = read_text_limited(note_path.as_path(), MAX_NOTE_CONTENT_BYTES).await?;
        Ok(json!({
            "ok": true,
            "note": self.note_output(&note),
            "content": content,
        }))
    }

    pub(super) async fn update_note_value(&self, params: Value) -> Result<Value, String> {
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
            write_atomic_limited(&new_path, content.as_bytes(), MAX_NOTE_CONTENT_BYTES).await?;
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

    pub(super) async fn delete_note_value(&self, id: &str) -> Result<Value, String> {
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
}

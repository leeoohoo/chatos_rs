use super::*;

impl TaskRunnerNotepadStore {
    pub(super) async fn init_store(&self) -> Result<Value, String> {
        self.ensure_initialized().await?;
        Ok(json!({
            "ok": true,
            "data_dir": self.data_dir.to_string_lossy(),
            "notes_root": self.notes_root.to_string_lossy(),
        }))
    }

    pub(super) async fn list_folders_value(&self) -> Result<Value, String> {
        let index = self.load_index().await?;
        let mut folders = BTreeSet::new();
        for note in index.notes {
            let mut current = String::new();
            for segment in super::super::support::folder_segments(note.folder.as_str()) {
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

    pub(super) async fn create_folder_value(&self, folder: &str) -> Result<Value, String> {
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

    pub(super) async fn rename_folder_value(&self, from: &str, to: &str) -> Result<Value, String> {
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

    pub(super) async fn delete_folder_value(
        &self,
        folder: &str,
        recursive: bool,
    ) -> Result<Value, String> {
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
}

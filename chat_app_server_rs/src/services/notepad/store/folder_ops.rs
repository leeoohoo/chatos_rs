use std::collections::HashSet;

use serde_json::{json, Value};
use tokio::fs;
use walkdir::WalkDir;

use super::super::store_normalize::{normalize_folder_path, now_iso, split_folder};
use super::super::types::TagCount;
use super::NotepadStore;

impl NotepadStore {
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

    pub async fn list_tags(&self) -> Result<Value, String> {
        let snapshot = self.get_index_snapshot().await?;
        let mut counts: std::collections::HashMap<String, TagCount> =
            std::collections::HashMap::new();

        for note in snapshot.notes {
            for tag in note.tags {
                let normalized = super::super::store_normalize::normalize_tag(tag.as_str());
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
}

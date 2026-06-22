use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::fs;
use tokio::sync::Mutex;

use super::support::{folder_segments, normalize_required, normalize_user_segment, write_atomic};
use super::{NoteMeta, NotesIndex, TaskRunnerNotepadStore};

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

    pub(super) async fn ensure_initialized(&self) -> Result<(), String> {
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

    pub(super) async fn load_index(&self) -> Result<NotesIndex, String> {
        self.ensure_initialized().await?;
        let bytes = fs::read(&self.index_path)
            .await
            .map_err(|err| err.to_string())?;
        if bytes.is_empty() {
            return Ok(NotesIndex::default());
        }
        serde_json::from_slice(&bytes).map_err(|err| err.to_string())
    }

    pub(super) async fn save_index(&self, index: &NotesIndex) -> Result<(), String> {
        self.ensure_initialized().await?;
        let text = serde_json::to_string_pretty(index).map_err(|err| err.to_string())?;
        write_atomic(&self.index_path, text.as_bytes()).await
    }

    pub(super) fn note_path(&self, folder: &str, id: &str) -> PathBuf {
        let mut path = self.notes_root.clone();
        for segment in folder_segments(folder) {
            path.push(segment);
        }
        path.push(format!("{id}.md"));
        path
    }

    pub(super) fn note_output(&self, note: &NoteMeta) -> Value {
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

    pub(super) async fn find_note(&self, id: &str) -> Result<NoteMeta, String> {
        let note_id = normalize_required(id, "id")?;
        let index = self.load_index().await?;
        index
            .notes
            .into_iter()
            .find(|note| note.id == note_id)
            .ok_or_else(|| format!("Note not found: {note_id}"))
    }
}

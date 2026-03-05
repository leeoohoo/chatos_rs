mod paths;
mod store;
mod types;

use std::collections::HashSet;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde_json::Value;

pub use types::{CreateNoteParams, ListNotesParams, SearchNotesParams, UpdateNoteParams};

use store::NotepadStore;

static MIGRATED_USERS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

#[derive(Clone)]
pub struct NotepadService {
    store: std::sync::Arc<NotepadStore>,
}

impl NotepadService {
    pub fn new(user_id: &str) -> Result<Self, String> {
        let user = user_id.trim();
        if user.is_empty() {
            return Err("user_id is required".to_string());
        }
        let data_dir = paths::resolve_data_dir(user);
        let should_migrate = {
            let mut guard = MIGRATED_USERS
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            guard.insert(user.to_string())
        };
        if should_migrate {
            if let Err(err) = paths::migrate_legacy_project_data(user, data_dir.as_path()) {
                tracing::warn!(
                    user_id = user,
                    error = %err,
                    "notepad legacy project migration failed"
                );
            }
        }
        Ok(Self {
            store: std::sync::Arc::new(NotepadStore::new(data_dir)),
        })
    }

    pub async fn init(&self) -> Result<Value, String> {
        self.store.init().await
    }

    pub async fn list_folders(&self) -> Result<Value, String> {
        self.store.list_folders().await
    }

    pub async fn create_folder(&self, folder: &str) -> Result<Value, String> {
        self.store.create_folder(folder).await
    }

    pub async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String> {
        self.store.rename_folder(from, to).await
    }

    pub async fn delete_folder(&self, folder: &str, recursive: bool) -> Result<Value, String> {
        self.store.delete_folder(folder, recursive).await
    }

    pub async fn list_notes(&self, params: ListNotesParams) -> Result<Value, String> {
        self.store.list_notes(params).await
    }

    pub async fn create_note(&self, params: CreateNoteParams) -> Result<Value, String> {
        self.store.create_note(params).await
    }

    pub async fn get_note(&self, id: &str) -> Result<Value, String> {
        self.store.get_note(id).await
    }

    pub async fn update_note(&self, params: UpdateNoteParams) -> Result<Value, String> {
        self.store.update_note(params).await
    }

    pub async fn delete_note(&self, id: &str) -> Result<Value, String> {
        self.store.delete_note(id).await
    }

    pub async fn list_tags(&self) -> Result<Value, String> {
        self.store.list_tags().await
    }

    pub async fn search_notes(&self, params: SearchNotesParams) -> Result<Value, String> {
        self.store.search_notes(params).await
    }
}

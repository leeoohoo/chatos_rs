use serde::{Deserialize, Serialize};

pub const INDEX_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteIndexEntry {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesIndex {
    pub version: i64,
    pub notes: Vec<NoteIndexEntry>,
}

impl Default for NotesIndex {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            notes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteOutput {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub file: String,
}

impl NoteOutput {
    pub fn from_entry(entry: &NoteIndexEntry) -> Self {
        let folder = entry.folder.trim().replace('\\', "/");
        let file = if folder.is_empty() {
            format!("notes/{}.md", entry.id)
        } else {
            format!("notes/{}/{}.md", folder, entry.id)
        };
        Self {
            id: entry.id.clone(),
            title: entry.title.clone(),
            folder: folder.clone(),
            tags: entry.tags.clone(),
            created_at: entry.created_at.clone(),
            updated_at: entry.updated_at.clone(),
            file,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListNotesParams {
    pub folder: String,
    pub recursive: bool,
    pub tags: Vec<String>,
    pub match_any: bool,
    pub query: String,
    pub limit: usize,
}

impl Default for ListNotesParams {
    fn default() -> Self {
        Self {
            folder: String::new(),
            recursive: true,
            tags: Vec::new(),
            match_any: false,
            query: String::new(),
            limit: 200,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateNoteParams {
    pub folder: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
}

impl Default for CreateNoteParams {
    fn default() -> Self {
        Self {
            folder: String::new(),
            title: String::new(),
            content: String::new(),
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateNoteParams {
    pub id: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub folder: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct SearchNotesParams {
    pub query: String,
    pub folder: String,
    pub recursive: bool,
    pub tags: Vec<String>,
    pub match_any: bool,
    pub include_content: bool,
    pub limit: usize,
}

impl Default for SearchNotesParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            folder: String::new(),
            recursive: true,
            tags: Vec::new(),
            match_any: false,
            include_content: true,
            limit: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

mod ops;
mod storage;
mod support;

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

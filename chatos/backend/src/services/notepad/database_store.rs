// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json;
use uuid::Uuid;

use super::store::{LegacyNotepadExport, NotepadStore};
use super::store_normalize::{
    extract_title_from_markdown, normalize_folder_path, normalize_string, normalize_title, now_iso,
    unique_tags,
};
use super::types::{
    CreateNoteParams, ListNotesParams, NoteIndexEntry, NoteOutput, SearchNotesParams, TagCount,
    UpdateNoteParams,
};

const MAX_NOTE_CONTENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredNote {
    id: String,
    user_id: String,
    title: String,
    folder: String,
    tags: Vec<String>,
    content: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    version: i64,
}

impl StoredNote {
    fn entry(&self) -> NoteIndexEntry {
        NoteIndexEntry {
            id: self.id.clone(),
            title: self.title.clone(),
            folder: self.folder.clone(),
            tags: self.tags.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    fn output(&self) -> NoteOutput {
        NoteOutput::from_entry(&self.entry())
    }
}

#[derive(Clone)]
pub(super) struct DatabaseNotepadStore {
    pub(super) user_id: String,
    pool: Option<chatos_postgres::PgPool>,
}

impl DatabaseNotepadStore {
    pub(super) fn new(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            pool: None,
        }
    }

    #[cfg(test)]
    fn with_pool(user_id: &str, pool: chatos_postgres::PgPool) -> Self {
        Self {
            user_id: user_id.to_string(),
            pool: Some(pool),
        }
    }

    async fn pool(&self) -> Result<chatos_postgres::PgPool, String> {
        if let Some(pool) = self.pool.as_ref() {
            return Ok(pool.clone());
        }
        Ok(crate::repositories::db::get_db().await?.pool.clone())
    }

    pub(super) async fn ensure_legacy_imported(
        &self,
        legacy_store: &NotepadStore,
    ) -> Result<(), String> {
        let pool = self.pool().await?;
        let migrated = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM notepad_storage_migrations WHERE user_id=$1)",
        )
        .bind(&self.user_id)
        .fetch_one(&pool)
        .await
        .map_err(db_error)?;
        if migrated {
            return Ok(());
        }

        let export = legacy_store.export_all().await?;
        self.import_legacy_export(&pool, export).await
    }

    async fn import_legacy_export(
        &self,
        pool: &chatos_postgres::PgPool,
        export: LegacyNotepadExport,
    ) -> Result<(), String> {
        let mut tx = pool.begin().await.map_err(db_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("chatos:notepad:legacy:{}", self.user_id))
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        let migrated = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM notepad_storage_migrations WHERE user_id=$1)",
        )
        .bind(&self.user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if migrated {
            tx.commit().await.map_err(db_error)?;
            return Ok(());
        }

        let mut folders = export.folders;
        for note in &export.notes {
            folders.extend(folder_ancestors(note.entry.folder.as_str()));
        }
        folders.sort();
        folders.dedup();
        let now = now_iso();
        let now_ts = crate::repositories::db::timestamp(&now)?;
        for folder in &folders {
            sqlx::query(
                "INSERT INTO notepad_folders(user_id,path,created_at,updated_at) VALUES($1,$2,$3,$3) ON CONFLICT(user_id,path) DO NOTHING",
            )
            .bind(&self.user_id)
            .bind(folder)
            .bind(now_ts)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        }

        let mut imported_notes = 0i64;
        for legacy in export.notes {
            ensure_content_size(&legacy.content)?;
            let note = StoredNote {
                id: legacy.entry.id,
                user_id: self.user_id.clone(),
                title: legacy.entry.title,
                folder: legacy.entry.folder,
                tags: legacy.entry.tags,
                content: legacy.content,
                created_at: legacy.entry.created_at,
                updated_at: legacy.entry.updated_at,
                deleted_at: None,
                version: 1,
            };
            let inserted = sqlx::query(
                "INSERT INTO notepad_notes(id,user_id,folder,updated_at,deleted_at,data) VALUES($1,$2,$3,$4,NULL,$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&note.id)
            .bind(&note.user_id)
            .bind(&note.folder)
            .bind(crate::repositories::db::timestamp(&note.updated_at)?)
            .bind(note_json(&note)?)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
            if inserted.rows_affected() == 1 {
                insert_revision(&mut tx, &note, "imported").await?;
                imported_notes += 1;
            }
        }

        sqlx::query(
            "INSERT INTO notepad_storage_migrations(user_id,source,imported_notes,imported_folders,completed_at) VALUES($1,'legacy-files',$2,$3,$4)",
        )
        .bind(&self.user_id)
        .bind(imported_notes)
        .bind(i64::try_from(folders.len()).unwrap_or(i64::MAX))
        .bind(now_ts)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)
    }

    pub(super) async fn init(&self) -> Result<Value, String> {
        let pool = self.pool().await?;
        let notes = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM notepad_notes WHERE user_id=$1 AND deleted_at IS NULL",
        )
        .bind(&self.user_id)
        .fetch_one(&pool)
        .await
        .map_err(db_error)?;
        Ok(json!({
            "ok": true,
            "storage": "postgres",
            "version": 2,
            "notes": notes
        }))
    }

    pub(super) async fn list_folders(&self) -> Result<Value, String> {
        let pool = self.pool().await?;
        let mut folders = sqlx::query_scalar::<_, String>(
            "SELECT path FROM notepad_folders WHERE user_id=$1 ORDER BY path",
        )
        .bind(&self.user_id)
        .fetch_all(&pool)
        .await
        .map_err(db_error)?;
        folders.insert(0, String::new());
        Ok(json!({"ok": true, "folders": folders}))
    }

    pub(super) async fn create_folder(&self, folder: &str) -> Result<Value, String> {
        let folder = normalize_folder_path(folder)?;
        if folder.is_empty() {
            return Err("folder is required".to_string());
        }
        let pool = self.pool().await?;
        let mut tx = pool.begin().await.map_err(db_error)?;
        insert_folders(&mut tx, &self.user_id, folder_ancestors(&folder)).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(json!({"ok": true, "folder": folder}))
    }

    pub(super) async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String> {
        let from = normalize_folder_path(from)?;
        let to = normalize_folder_path(to)?;
        if from.is_empty() {
            return Err("from is required".to_string());
        }
        if to.is_empty() {
            return Err("to is required".to_string());
        }
        if from == to {
            return Ok(json!({"ok": true, "from": from, "to": to, "moved_notes": 0}));
        }

        let pool = self.pool().await?;
        let mut tx = pool.begin().await.map_err(db_error)?;
        let folders = sqlx::query_scalar::<_, String>(
            "SELECT path FROM notepad_folders WHERE user_id=$1 AND (path=$2 OR path LIKE $3) ORDER BY path FOR UPDATE",
        )
        .bind(&self.user_id)
        .bind(&from)
        .bind(format!("{from}/%"))
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        if !folders.iter().any(|path| path == &from) {
            return Err(format!("Folder not found: {from}"));
        }
        let target_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM notepad_folders WHERE user_id=$1 AND path=$2)",
        )
        .bind(&self.user_id)
        .bind(&to)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if target_exists {
            return Err(format!("Target folder already exists: {to}"));
        }

        let notes = fetch_notes_for_folder(&mut tx, &self.user_id, &from, true).await?;
        sqlx::query("DELETE FROM notepad_folders WHERE user_id=$1 AND (path=$2 OR path LIKE $3)")
            .bind(&self.user_id)
            .bind(&from)
            .bind(format!("{from}/%"))
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        insert_folders(&mut tx, &self.user_id, folder_ancestors(&to)).await?;
        for folder in folders {
            let renamed = replace_folder_prefix(&folder, &from, &to);
            insert_folders(&mut tx, &self.user_id, vec![renamed]).await?;
        }

        let now = now_iso();
        for mut note in notes.iter().cloned() {
            note.folder = replace_folder_prefix(&note.folder, &from, &to);
            note.updated_at = now.clone();
            note.version += 1;
            update_note_row(&mut tx, &note).await?;
            insert_revision(&mut tx, &note, "updated").await?;
        }
        tx.commit().await.map_err(db_error)?;
        Ok(json!({
            "ok": true,
            "from": from,
            "to": to,
            "moved_notes": notes.len()
        }))
    }

    pub(super) async fn delete_folder(
        &self,
        folder: &str,
        recursive: bool,
    ) -> Result<Value, String> {
        let folder = normalize_folder_path(folder)?;
        if folder.is_empty() {
            return Err("folder is required".to_string());
        }
        let pool = self.pool().await?;
        let mut tx = pool.begin().await.map_err(db_error)?;
        let folders = sqlx::query_scalar::<_, String>(
            "SELECT path FROM notepad_folders WHERE user_id=$1 AND (path=$2 OR path LIKE $3) ORDER BY path FOR UPDATE",
        )
        .bind(&self.user_id)
        .bind(&folder)
        .bind(format!("{folder}/%"))
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        if !folders.iter().any(|path| path == &folder) {
            return Err(format!("Folder not found: {folder}"));
        }
        let notes = fetch_notes_for_folder(&mut tx, &self.user_id, &folder, true).await?;
        if !recursive && (folders.len() > 1 || !notes.is_empty()) {
            return Err(format!("Folder is not empty: {folder}"));
        }

        let now = now_iso();
        if recursive {
            for mut note in notes.iter().cloned() {
                note.deleted_at = Some(now.clone());
                note.updated_at = now.clone();
                note.version += 1;
                update_note_row(&mut tx, &note).await?;
                insert_revision(&mut tx, &note, "deleted").await?;
            }
            sqlx::query(
                "DELETE FROM notepad_folders WHERE user_id=$1 AND (path=$2 OR path LIKE $3)",
            )
            .bind(&self.user_id)
            .bind(&folder)
            .bind(format!("{folder}/%"))
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        } else {
            sqlx::query("DELETE FROM notepad_folders WHERE user_id=$1 AND path=$2")
                .bind(&self.user_id)
                .bind(&folder)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
        }
        tx.commit().await.map_err(db_error)?;
        Ok(json!({"ok": true, "folder": folder, "deleted_notes": notes.len()}))
    }

    pub(super) async fn list_notes(&self, params: ListNotesParams) -> Result<Value, String> {
        let mut notes = self.active_notes().await?;
        apply_note_filters(&mut notes, &params)?;
        notes.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let output: Vec<_> = notes
            .iter()
            .take(params.limit.clamp(1, 500))
            .map(StoredNote::output)
            .collect();
        Ok(json!({"ok": true, "notes": output}))
    }

    pub(super) async fn create_note(&self, params: CreateNoteParams) -> Result<Value, String> {
        let folder = normalize_folder_path(&params.folder)?;
        let title_input = normalize_title(&params.title);
        let title_content = normalize_title(&extract_title_from_markdown(&params.content));
        let title = if !title_input.is_empty() {
            title_input
        } else if !title_content.is_empty() {
            title_content
        } else {
            "Untitled".to_string()
        };
        let content = if normalize_string(&params.content).is_empty() {
            format!("# {title}\n\n")
        } else {
            params.content
        };
        ensure_content_size(&content)?;
        let now = now_iso();
        let note = StoredNote {
            id: Uuid::new_v4().to_string(),
            user_id: self.user_id.clone(),
            title,
            folder,
            tags: unique_tags(&params.tags),
            content,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
            version: 1,
        };

        let pool = self.pool().await?;
        let mut tx = pool.begin().await.map_err(db_error)?;
        insert_folders(&mut tx, &self.user_id, folder_ancestors(&note.folder)).await?;
        sqlx::query(
            "INSERT INTO notepad_notes(id,user_id,folder,updated_at,deleted_at,data) VALUES($1,$2,$3,$4,NULL,$5)",
        )
        .bind(&note.id)
        .bind(&note.user_id)
        .bind(&note.folder)
        .bind(crate::repositories::db::timestamp(&note.updated_at)?)
        .bind(note_json(&note)?)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        insert_revision(&mut tx, &note, "created").await?;
        tx.commit().await.map_err(db_error)?;
        Ok(json!({"ok": true, "note": note.output()}))
    }

    pub(super) async fn get_note(&self, id: &str) -> Result<Value, String> {
        let id = required_note_id(id)?;
        let pool = self.pool().await?;
        let note = fetch_note(&pool, &self.user_id, &id).await?;
        Ok(json!({"ok": true, "note": note.output(), "content": note.content}))
    }

    pub(super) async fn update_note(&self, params: UpdateNoteParams) -> Result<Value, String> {
        let id = required_note_id(&params.id)?;
        let pool = self.pool().await?;
        let mut tx = pool.begin().await.map_err(db_error)?;
        let mut note = fetch_note_for_update(&mut tx, &self.user_id, &id).await?;
        if let Some(folder) = params.folder {
            note.folder = normalize_folder_path(&folder)?;
            insert_folders(&mut tx, &self.user_id, folder_ancestors(&note.folder)).await?;
        }
        if let Some(title) = params.title {
            let title = normalize_title(&title);
            if !title.is_empty() {
                note.title = title;
            }
        }
        if let Some(tags) = params.tags {
            note.tags = unique_tags(&tags);
        }
        if let Some(content) = params.content {
            ensure_content_size(&content)?;
            note.content = content;
        }
        note.updated_at = now_iso();
        note.version += 1;
        update_note_row(&mut tx, &note).await?;
        insert_revision(&mut tx, &note, "updated").await?;
        tx.commit().await.map_err(db_error)?;
        Ok(json!({"ok": true, "note": note.output()}))
    }

    pub(super) async fn delete_note(&self, id: &str) -> Result<Value, String> {
        let id = required_note_id(id)?;
        let pool = self.pool().await?;
        let mut tx = pool.begin().await.map_err(db_error)?;
        let mut note = fetch_note_for_update(&mut tx, &self.user_id, &id).await?;
        let now = now_iso();
        note.deleted_at = Some(now.clone());
        note.updated_at = now;
        note.version += 1;
        update_note_row(&mut tx, &note).await?;
        insert_revision(&mut tx, &note, "deleted").await?;
        tx.commit().await.map_err(db_error)?;
        Ok(json!({"ok": true, "id": id}))
    }

    pub(super) async fn list_tags(&self) -> Result<Value, String> {
        let mut counts: HashMap<String, TagCount> = HashMap::new();
        for note in self.active_notes().await? {
            for tag in note.tags {
                let normalized = super::store_normalize::normalize_tag(&tag);
                if normalized.is_empty() {
                    continue;
                }
                counts
                    .entry(normalized.to_lowercase())
                    .and_modify(|item| item.count += 1)
                    .or_insert(TagCount {
                        tag: normalized,
                        count: 1,
                    });
            }
        }
        let mut tags: Vec<_> = counts.into_values().collect();
        tags.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then_with(|| left.tag.to_lowercase().cmp(&right.tag.to_lowercase()))
        });
        Ok(json!({"ok": true, "tags": tags}))
    }

    pub(super) async fn search_notes(&self, params: SearchNotesParams) -> Result<Value, String> {
        let query = normalize_string(&params.query);
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        let list_params = ListNotesParams {
            folder: params.folder,
            recursive: params.recursive,
            tags: params.tags,
            match_any: params.match_any,
            query: String::new(),
            limit: 500,
        };
        let mut notes = self.active_notes().await?;
        apply_note_filters(&mut notes, &list_params)?;
        notes.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let query = query.to_lowercase();
        let results: Vec<_> = notes
            .into_iter()
            .filter(|note| {
                note.title.to_lowercase().contains(&query)
                    || note.folder.to_lowercase().contains(&query)
                    || (params.include_content && note.content.to_lowercase().contains(&query))
            })
            .take(params.limit.clamp(1, 200))
            .map(|note| note.output())
            .collect();
        Ok(json!({"ok": true, "notes": results}))
    }

    async fn active_notes(&self) -> Result<Vec<StoredNote>, String> {
        let pool = self.pool().await?;
        let values = sqlx::query_scalar::<_, Json<Value>>(
            "SELECT data FROM notepad_notes WHERE user_id=$1 AND deleted_at IS NULL ORDER BY updated_at DESC,id",
        )
        .bind(&self.user_id)
        .fetch_all(&pool)
        .await
        .map_err(db_error)?;
        values
            .into_iter()
            .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
            .collect()
    }
}

fn apply_note_filters(notes: &mut Vec<StoredNote>, params: &ListNotesParams) -> Result<(), String> {
    let folder = normalize_folder_path(&params.folder)?;
    let desired_tags = unique_tags(&params.tags);
    let query = normalize_string(&params.query).to_lowercase();
    notes.retain(|note| {
        if !folder.is_empty()
            && note.folder != folder
            && (!params.recursive || !note.folder.starts_with(&format!("{folder}/")))
        {
            return false;
        }
        if !desired_tags.is_empty() {
            let note_tags: HashSet<_> = note.tags.iter().map(|tag| tag.to_lowercase()).collect();
            let matches = if params.match_any {
                desired_tags
                    .iter()
                    .any(|tag| note_tags.contains(&tag.to_lowercase()))
            } else {
                desired_tags
                    .iter()
                    .all(|tag| note_tags.contains(&tag.to_lowercase()))
            };
            if !matches {
                return false;
            }
        }
        query.is_empty()
            || note.title.to_lowercase().contains(&query)
            || note.folder.to_lowercase().contains(&query)
    });
    Ok(())
}

fn folder_ancestors(folder: &str) -> Vec<String> {
    let mut current = String::new();
    folder
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            current.clone()
        })
        .collect()
}

fn replace_folder_prefix(folder: &str, from: &str, to: &str) -> String {
    if folder == from {
        return to.to_string();
    }
    folder
        .strip_prefix(&format!("{from}/"))
        .map(|suffix| format!("{to}/{suffix}"))
        .unwrap_or_else(|| folder.to_string())
}

fn required_note_id(id: &str) -> Result<String, String> {
    let id = normalize_string(id);
    if id.is_empty() {
        Err("id is required".to_string())
    } else {
        Ok(id)
    }
}

fn ensure_content_size(content: &str) -> Result<(), String> {
    if content.len() > MAX_NOTE_CONTENT_BYTES {
        return Err(format!(
            "notepad content exceeds limit: {} bytes > {} bytes",
            content.len(),
            MAX_NOTE_CONTENT_BYTES
        ));
    }
    Ok(())
}

fn note_json(note: &StoredNote) -> Result<Json<Value>, String> {
    serde_json::to_value(note)
        .map(Json)
        .map_err(|err| err.to_string())
}

fn db_error(error: sqlx::Error) -> String {
    error.to_string()
}

async fn insert_folders(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    folders: Vec<String>,
) -> Result<(), String> {
    for folder in folders {
        if folder.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO notepad_folders(user_id,path,created_at,updated_at) VALUES($1,$2,now(),now()) ON CONFLICT(user_id,path) DO NOTHING",
        )
        .bind(user_id)
        .bind(folder)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    }
    Ok(())
}

async fn fetch_note(
    pool: &chatos_postgres::PgPool,
    user_id: &str,
    id: &str,
) -> Result<StoredNote, String> {
    let value = sqlx::query_scalar::<_, Json<Value>>(
        "SELECT data FROM notepad_notes WHERE id=$1 AND user_id=$2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| format!("Note not found: {id}"))?;
    serde_json::from_value(value.0).map_err(|err| err.to_string())
}

async fn fetch_note_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    id: &str,
) -> Result<StoredNote, String> {
    let value = sqlx::query_scalar::<_, Json<Value>>(
        "SELECT data FROM notepad_notes WHERE id=$1 AND user_id=$2 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?
    .ok_or_else(|| format!("Note not found: {id}"))?;
    serde_json::from_value(value.0).map_err(|err| err.to_string())
}

async fn fetch_notes_for_folder(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    folder: &str,
    recursive: bool,
) -> Result<Vec<StoredNote>, String> {
    let values = if recursive {
        sqlx::query_scalar::<_, Json<Value>>(
            "SELECT data FROM notepad_notes WHERE user_id=$1 AND deleted_at IS NULL AND (folder=$2 OR folder LIKE $3) FOR UPDATE",
        )
        .bind(user_id)
        .bind(folder)
        .bind(format!("{folder}/%"))
        .fetch_all(&mut **tx)
        .await
        .map_err(db_error)?
    } else {
        sqlx::query_scalar::<_, Json<Value>>(
            "SELECT data FROM notepad_notes WHERE user_id=$1 AND deleted_at IS NULL AND folder=$2 FOR UPDATE",
        )
        .bind(user_id)
        .bind(folder)
        .fetch_all(&mut **tx)
        .await
        .map_err(db_error)?
    };
    values
        .into_iter()
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .collect()
}

async fn update_note_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    note: &StoredNote,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE notepad_notes SET folder=$1,updated_at=$2,deleted_at=$3,data=$4 WHERE id=$5 AND user_id=$6",
    )
    .bind(&note.folder)
    .bind(crate::repositories::db::timestamp(&note.updated_at)?)
    .bind(
        note.deleted_at
            .as_deref()
            .map(crate::repositories::db::timestamp)
            .transpose()?,
    )
    .bind(note_json(note)?)
    .bind(&note.id)
    .bind(&note.user_id)
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn insert_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    note: &StoredNote,
    action: &str,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO notepad_note_revisions(note_id,user_id,version,action,created_at,data) VALUES($1,$2,$3,$4,now(),$5)",
    )
    .bind(&note.id)
    .bind(&note.user_id)
    .bind(note.version)
    .bind(action)
    .bind(note_json(note)?)
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

#[cfg(test)]
mod tests;

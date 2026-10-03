// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::{LocalNotepadNote, LocalNotepadNoteDetail};
use sqlx::{Row, SqliteConnection};

pub(super) const NOTE_SELECT: &str =
    "SELECT owner_user_id, note_id, title, folder, content, tags_json, \
     version, created_at_unix_ms, updated_at_unix_ms FROM local_notepad_notes";

pub(super) async fn fetch_note(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    note_id: &str,
) -> Result<Option<LocalNotepadNoteDetail>, ClientStorageError> {
    let row = sqlx::query(&format!(
        "{NOTE_SELECT} WHERE owner_user_id = ? AND note_id = ?"
    ))
    .bind(owner_user_id)
    .bind(note_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    row.map(|row| {
        let content: String = row.try_get("content").db()?;
        Ok(LocalNotepadNoteDetail {
            note: decode_note(row)?,
            content,
        })
    })
    .transpose()
}

pub(super) fn decode_note(
    row: sqlx::sqlite::SqliteRow,
) -> Result<LocalNotepadNote, ClientStorageError> {
    let note_id: String = row.try_get("note_id").db()?;
    let version: i64 = row.try_get("version").db()?;
    let tags_json: String = row.try_get("tags_json").db()?;
    Ok(LocalNotepadNote {
        file: format!("{note_id}.md"),
        note_id,
        owner_user_id: row.try_get("owner_user_id").db()?,
        title: row.try_get("title").db()?,
        folder: row.try_get("folder").db()?,
        tags: serde_json::from_str(&tags_json)?,
        version: u64::try_from(version).map_err(|_| {
            ClientStorageError::InvalidState("notepad note version is invalid".to_string())
        })?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

pub(super) async fn insert_folder_ancestors(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    folder: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if folder.is_empty() {
        return Ok(());
    }
    let segments: Vec<_> = folder.split('/').collect();
    for end in 1..=segments.len() {
        let path = segments[..end].join("/");
        sqlx::query(
            "INSERT INTO local_notepad_folders(\
             owner_user_id, path, created_at_unix_ms, updated_at_unix_ms) \
             VALUES(?, ?, ?, ?) ON CONFLICT(owner_user_id, path) DO NOTHING",
        )
        .bind(owner_user_id)
        .bind(path)
        .bind(now_unix_ms)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
    }
    Ok(())
}

pub(super) async fn folder_exists(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    folder: &str,
) -> Result<bool, ClientStorageError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_notepad_folders WHERE owner_user_id = ? AND path = ?",
    )
    .bind(owner_user_id)
    .bind(folder)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    Ok(count == 1)
}

pub(super) async fn ensure_folder_exists(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    folder: &str,
) -> Result<(), ClientStorageError> {
    if folder_exists(connection, owner_user_id, folder).await? {
        Ok(())
    } else {
        Err(ClientStorageError::NotFound(folder.to_string()))
    }
}

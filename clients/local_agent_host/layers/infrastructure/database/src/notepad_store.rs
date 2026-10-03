// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::notepad_support::{
    decode_note, ensure_folder_exists, fetch_note, folder_exists, insert_folder_ancestors,
    NOTE_SELECT,
};
use super::{
    is_unique_violation, ClientStorageError, IdempotentCommand, LocalNotepadImageWrite,
    LocalNotepadStore, SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalNotepadImage, LocalNotepadNote, LocalNotepadNoteDetail, UpdateNotepadNoteCommand,
};

#[async_trait]
impl LocalNotepadStore for SqliteClientStorage {
    async fn initialize_notepad(&self, owner_user_id: &str) -> Result<u64, ClientStorageError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM local_notepad_notes WHERE owner_user_id = ?")
                .bind(owner_user_id)
                .fetch_one(&self.pool)
                .await
                .db()?;
        u64::try_from(count).map_err(|_| {
            ClientStorageError::InvalidState("notepad note count is invalid".to_string())
        })
    }

    async fn list_notepad_folders(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<String>, ClientStorageError> {
        let mut folders = sqlx::query_scalar::<_, String>(
            "SELECT path FROM local_notepad_folders WHERE owner_user_id = ? ORDER BY path",
        )
        .bind(owner_user_id)
        .fetch_all(&self.pool)
        .await
        .db()?;
        folders.insert(0, String::new());
        Ok(folders)
    }

    async fn create_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        folder: &str,
        now_unix_ms: i64,
    ) -> Result<String, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            insert_folder_ancestors(&mut connection, owner_user_id, folder, now_unix_ms).await?;
            let response = folder.to_string();
            Self::record_receipt(&mut connection, command, &response, now_unix_ms).await?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn rename_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        from: &str,
        to: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            ensure_folder_exists(&mut connection, owner_user_id, from).await?;
            if folder_exists(&mut connection, owner_user_id, to).await? {
                return Err(ClientStorageError::Conflict(format!(
                    "target notepad folder already exists: {to}"
                )));
            }
            insert_folder_ancestors(&mut connection, owner_user_id, to, now_unix_ms).await?;
            sqlx::query(
                "INSERT INTO local_notepad_folders(\
                 owner_user_id, path, created_at_unix_ms, updated_at_unix_ms) \
                 SELECT owner_user_id, \
                   CASE WHEN path = ? THEN ? ELSE ? || substr(path, length(?) + 1) END, \
                   ?, ? FROM local_notepad_folders WHERE owner_user_id = ? \
                   AND (path = ? OR instr(path, ? || '/') = 1) \
                 ON CONFLICT(owner_user_id, path) DO NOTHING",
            )
            .bind(from)
            .bind(to)
            .bind(to)
            .bind(from)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .bind(owner_user_id)
            .bind(from)
            .bind(from)
            .execute(&mut *connection)
            .await
            .db()?;
            sqlx::query(
                "DELETE FROM local_notepad_folders WHERE owner_user_id = ? \
                 AND (path = ? OR instr(path, ? || '/') = 1)",
            )
            .bind(owner_user_id)
            .bind(from)
            .bind(from)
            .execute(&mut *connection)
            .await
            .db()?;
            let affected = sqlx::query(
                "UPDATE local_notepad_notes SET folder = CASE \
                   WHEN folder = ? THEN ? ELSE ? || substr(folder, length(?) + 1) END, \
                 version = version + 1, updated_at_unix_ms = ? \
                 WHERE owner_user_id = ? \
                 AND (folder = ? OR instr(folder, ? || '/') = 1)",
            )
            .bind(from)
            .bind(to)
            .bind(to)
            .bind(from)
            .bind(now_unix_ms)
            .bind(owner_user_id)
            .bind(from)
            .bind(from)
            .execute(&mut *connection)
            .await
            .db()?
            .rows_affected();
            Self::record_receipt(&mut connection, command, &affected, now_unix_ms).await?;
            Ok(affected)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn delete_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        folder: &str,
        recursive: bool,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            ensure_folder_exists(&mut connection, owner_user_id, folder).await?;
            let descendant_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM local_notepad_folders WHERE owner_user_id = ? \
                 AND instr(path, ? || '/') = 1",
            )
            .bind(owner_user_id)
            .bind(folder)
            .fetch_one(&mut *connection)
            .await
            .db()?;
            let note_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM local_notepad_notes WHERE owner_user_id = ? \
                 AND (folder = ? OR instr(folder, ? || '/') = 1)",
            )
            .bind(owner_user_id)
            .bind(folder)
            .bind(folder)
            .fetch_one(&mut *connection)
            .await
            .db()?;
            if !recursive && (descendant_count > 0 || note_count > 0) {
                return Err(ClientStorageError::Conflict(format!(
                    "notepad folder is not empty: {folder}"
                )));
            }
            if recursive {
                sqlx::query(
                    "DELETE FROM local_notepad_notes WHERE owner_user_id = ? \
                     AND (folder = ? OR instr(folder, ? || '/') = 1)",
                )
                .bind(owner_user_id)
                .bind(folder)
                .bind(folder)
                .execute(&mut *connection)
                .await
                .db()?;
                sqlx::query(
                    "DELETE FROM local_notepad_folders WHERE owner_user_id = ? \
                     AND (path = ? OR instr(path, ? || '/') = 1)",
                )
                .bind(owner_user_id)
                .bind(folder)
                .bind(folder)
                .execute(&mut *connection)
                .await
                .db()?;
            } else {
                sqlx::query(
                    "DELETE FROM local_notepad_folders WHERE owner_user_id = ? AND path = ?",
                )
                .bind(owner_user_id)
                .bind(folder)
                .execute(&mut *connection)
                .await
                .db()?;
            }
            let affected = u64::try_from(note_count).map_err(|_| {
                ClientStorageError::InvalidState("notepad note count is invalid".to_string())
            })?;
            Self::record_receipt(&mut connection, command, &affected, now_unix_ms).await?;
            Ok(affected)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn list_notepad_notes(
        &self,
        owner_user_id: &str,
        query: Option<&str>,
        limit: u32,
    ) -> Result<Vec<LocalNotepadNote>, ClientStorageError> {
        let rows = match query.filter(|value| !value.is_empty()) {
            Some(query) => sqlx::query(&format!(
                "{NOTE_SELECT} WHERE owner_user_id = ? AND (\
                     instr(lower(title), lower(?)) > 0 OR instr(lower(folder), lower(?)) > 0 OR \
                     instr(lower(content), lower(?)) > 0 OR instr(lower(tags_json), lower(?)) > 0) \
                     ORDER BY updated_at_unix_ms DESC, note_id LIMIT ?"
            ))
            .bind(owner_user_id)
            .bind(query)
            .bind(query)
            .bind(query)
            .bind(query)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .db()?,
            None => sqlx::query(&format!(
                "{NOTE_SELECT} WHERE owner_user_id = ? \
                     ORDER BY updated_at_unix_ms DESC, note_id LIMIT ?"
            ))
            .bind(owner_user_id)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .db()?,
        };
        rows.into_iter().map(decode_note).collect()
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_notepad_note(
        &self,
        command: &IdempotentCommand,
        note_id: &str,
        owner_user_id: &str,
        folder: &str,
        title: &str,
        content: &str,
        tags: &[String],
        now_unix_ms: i64,
    ) -> Result<LocalNotepadNoteDetail, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            insert_folder_ancestors(&mut connection, owner_user_id, folder, now_unix_ms).await?;
            let inserted = sqlx::query(
                "INSERT INTO local_notepad_notes(\
                 owner_user_id, note_id, title, folder, content, tags_json, version, \
                 created_at_unix_ms, updated_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(owner_user_id)
            .bind(note_id)
            .bind(title)
            .bind(folder)
            .bind(content)
            .bind(serde_json::to_string(tags)?)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await;
            match inserted {
                Ok(_) => {}
                Err(error) if is_unique_violation(&error) => {
                    return Err(ClientStorageError::Conflict(format!(
                        "notepad note already exists: {note_id}"
                    )))
                }
                Err(error) => return Err(ClientStorageError::database(error)),
            }
            let detail = fetch_note(&mut connection, owner_user_id, note_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(note_id.to_string()))?;
            Self::record_receipt(&mut connection, command, &detail, now_unix_ms).await?;
            Ok(detail)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_notepad_note(
        &self,
        owner_user_id: &str,
        note_id: &str,
    ) -> Result<Option<LocalNotepadNoteDetail>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_note(&mut connection, owner_user_id, note_id).await
    }

    async fn update_notepad_note(
        &self,
        command: &IdempotentCommand,
        update: &UpdateNotepadNoteCommand,
        now_unix_ms: i64,
    ) -> Result<LocalNotepadNoteDetail, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let current = fetch_note(&mut connection, &update.owner_user_id, &update.note_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(update.note_id.clone()))?;
            if current.note.version != update.expected_version {
                return Err(ClientStorageError::Conflict(format!(
                    "notepad note version changed: {}",
                    update.note_id
                )));
            }
            let folder = update.folder.as_deref().unwrap_or(&current.note.folder);
            insert_folder_ancestors(&mut connection, &update.owner_user_id, folder, now_unix_ms)
                .await?;
            let title = update.title.as_deref().unwrap_or(&current.note.title);
            let content = update.content.as_deref().unwrap_or(&current.content);
            let tags = update.tags.as_deref().unwrap_or(&current.note.tags);
            let changed = sqlx::query(
                "UPDATE local_notepad_notes SET title = ?, folder = ?, content = ?, \
                 tags_json = ?, version = version + 1, updated_at_unix_ms = ? \
                 WHERE owner_user_id = ? AND note_id = ? AND version = ?",
            )
            .bind(title)
            .bind(folder)
            .bind(content)
            .bind(serde_json::to_string(tags)?)
            .bind(now_unix_ms)
            .bind(&update.owner_user_id)
            .bind(&update.note_id)
            .bind(i64::try_from(update.expected_version).map_err(|_| {
                ClientStorageError::InvalidState("notepad note version exceeds i64".to_string())
            })?)
            .execute(&mut *connection)
            .await
            .db()?;
            if changed.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "notepad note changed while updating: {}",
                    update.note_id
                )));
            }
            let detail = fetch_note(&mut connection, &update.owner_user_id, &update.note_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(update.note_id.clone()))?;
            Self::record_receipt(&mut connection, command, &detail, now_unix_ms).await?;
            Ok(detail)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn delete_notepad_note(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        note_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if Self::replay::<String>(&mut connection, command).await?.is_some() {
                return Ok(());
            }
            let deleted = sqlx::query(
                "DELETE FROM local_notepad_notes WHERE owner_user_id = ? AND note_id = ? \
                 AND version = ?",
            )
            .bind(owner_user_id)
            .bind(note_id)
            .bind(i64::try_from(expected_version).map_err(|_| {
                ClientStorageError::InvalidState("notepad note version exceeds i64".to_string())
            })?)
            .execute(&mut *connection)
            .await
            .db()?;
            if deleted.rows_affected() != 1 {
                let exists: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM local_notepad_notes WHERE owner_user_id = ? AND note_id = ?",
                )
                .bind(owner_user_id)
                .bind(note_id)
                .fetch_one(&mut *connection)
                .await
                .db()?;
                return Err(if exists == 0 {
                    ClientStorageError::NotFound(note_id.to_string())
                } else {
                    ClientStorageError::Conflict(format!("notepad note version changed: {note_id}"))
                });
            }
            Self::record_receipt(&mut connection, command, &note_id.to_string(), now_unix_ms)
                .await?;
            Ok(())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn put_notepad_image(
        &self,
        command: &IdempotentCommand,
        write: &LocalNotepadImageWrite,
    ) -> Result<LocalNotepadImage, ClientStorageError> {
        let image = &write.image;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            if fetch_note(&mut connection, &image.owner_user_id, &image.note_id)
                .await?
                .is_none()
            {
                return Err(ClientStorageError::NotFound(image.note_id.clone()));
            }
            sqlx::query(
                "INSERT INTO local_notepad_images(\
                 owner_user_id, image_id, note_id, name, mime_type, size, sha256, data, \
                 created_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&image.owner_user_id)
            .bind(&image.image_id)
            .bind(&image.note_id)
            .bind(&image.name)
            .bind(&image.mime_type)
            .bind(i64::try_from(image.size).map_err(|_| {
                ClientStorageError::InvalidState("notepad image size exceeds i64".to_string())
            })?)
            .bind(&image.sha256)
            .bind(&write.data)
            .bind(image.created_at_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            Self::record_receipt(&mut connection, command, image, image.created_at_unix_ms).await?;
            Ok(image.clone())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

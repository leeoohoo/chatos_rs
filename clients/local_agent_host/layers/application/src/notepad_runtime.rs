// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand, LocalNotepadImageWrite};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalNotepadImage, LOCAL_NOTEPAD_MAX_IMAGE_BYTES,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

impl LocalAgentRuntime {
    pub(super) async fn handle_notepad_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::InitializeNotepad(command) => {
                let note_count = self
                    .store
                    .initialize_notepad(&command.owner_user_id)
                    .await?;
                Ok(HostResult::NotepadInitialized { note_count })
            }
            HostCommand::ListNotepadFolders(command) => {
                let folders = self
                    .store
                    .list_notepad_folders(&command.owner_user_id)
                    .await?;
                Ok(HostResult::NotepadFolders { folders })
            }
            HostCommand::CreateNotepadFolder(command) => {
                let folder = normalize_folder(&command.folder, false)?;
                let folder = self
                    .store
                    .create_notepad_folder(
                        idempotency,
                        &command.owner_user_id,
                        &folder,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::NotepadFolderMutation {
                    folder,
                    affected_notes: 0,
                })
            }
            HostCommand::RenameNotepadFolder(command) => {
                let from = normalize_folder(&command.from, false)?;
                let to = normalize_folder(&command.to, false)?;
                if from == to {
                    return Ok(HostResult::NotepadFolderMutation {
                        folder: to,
                        affected_notes: 0,
                    });
                }
                if to.starts_with(&format!("{from}/")) {
                    return Err(LocalAgentRuntimeError::InvalidRequest(
                        "a notepad folder cannot be moved into itself".to_string(),
                    ));
                }
                let affected_notes = self
                    .store
                    .rename_notepad_folder(
                        idempotency,
                        &command.owner_user_id,
                        &from,
                        &to,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::NotepadFolderMutation {
                    folder: to,
                    affected_notes,
                })
            }
            HostCommand::DeleteNotepadFolder(command) => {
                let folder = normalize_folder(&command.folder, false)?;
                let affected_notes = self
                    .store
                    .delete_notepad_folder(
                        idempotency,
                        &command.owner_user_id,
                        &folder,
                        command.recursive,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::NotepadFolderMutation {
                    folder,
                    affected_notes,
                })
            }
            HostCommand::ListNotepadNotes(command) => {
                let query = command
                    .query
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let notes = self
                    .store
                    .list_notepad_notes(&command.owner_user_id, query, command.limit)
                    .await?;
                Ok(HostResult::NotepadNotes { notes })
            }
            HostCommand::CreateNotepadNote(command) => {
                let folder = normalize_folder(&command.folder, true)?;
                let title = normalize_title(&command.title, &command.content);
                let content = if command.content.trim().is_empty() {
                    format!("# {title}\n\n")
                } else {
                    command.content
                };
                let tags = normalize_tags(command.tags);
                let detail = self
                    .store
                    .create_notepad_note(
                        idempotency,
                        &Uuid::new_v4().to_string(),
                        &command.owner_user_id,
                        &folder,
                        &title,
                        &content,
                        &tags,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::NotepadNote { detail })
            }
            HostCommand::GetNotepadNote(command) => {
                let detail = self
                    .store
                    .get_notepad_note(&command.owner_user_id, &command.note_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.note_id))?;
                Ok(HostResult::NotepadNote { detail })
            }
            HostCommand::UpdateNotepadNote(mut command) => {
                if let Some(folder) = command.folder.as_deref() {
                    command.folder = Some(normalize_folder(folder, true)?);
                }
                if let Some(title) = command.title.as_deref() {
                    command.title = Some(normalize_updated_title(title)?);
                }
                if let Some(tags) = command.tags.take() {
                    command.tags = Some(normalize_tags(tags));
                }
                let detail = self
                    .store
                    .update_notepad_note(idempotency, &command, self.now()?)
                    .await?;
                Ok(HostResult::NotepadNote { detail })
            }
            HostCommand::DeleteNotepadNote(command) => {
                self.store
                    .delete_notepad_note(
                        idempotency,
                        &command.owner_user_id,
                        &command.note_id,
                        command.expected_version,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::NotepadNoteDeleted {
                    note_id: command.note_id,
                })
            }
            HostCommand::PutNotepadImage(command) => {
                let data = STANDARD.decode(&command.data_base64).map_err(|_| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "notepad image data is not valid base64".to_string(),
                    )
                })?;
                if data.is_empty() || data.len() > LOCAL_NOTEPAD_MAX_IMAGE_BYTES {
                    return Err(LocalAgentRuntimeError::InvalidRequest(format!(
                        "notepad image must be 1..={LOCAL_NOTEPAD_MAX_IMAGE_BYTES} bytes"
                    )));
                }
                validate_image_signature(&command.mime_type, &data)?;
                let digest = format!("{:x}", Sha256::digest(&data));
                let data_url = format!("data:{};base64,{}", command.mime_type, command.data_base64);
                let image = LocalNotepadImage {
                    image_id: Uuid::new_v4().to_string(),
                    note_id: command.note_id,
                    owner_user_id: command.owner_user_id,
                    name: sanitize_image_name(&command.name),
                    mime_type: command.mime_type,
                    size: u64::try_from(data.len()).map_err(|_| {
                        LocalAgentRuntimeError::InvalidRequest(
                            "notepad image size exceeds u64".to_string(),
                        )
                    })?,
                    sha256: digest,
                    data_url,
                    created_at_unix_ms: self.now()?,
                };
                let image = self
                    .store
                    .put_notepad_image(idempotency, &LocalNotepadImageWrite { image, data })
                    .await?;
                Ok(HostResult::NotepadImage { image })
            }
            _ => unreachable!("non-notepad command routed to notepad runtime"),
        }
    }
}

fn normalize_folder(value: &str, allow_empty: bool) -> Result<String, LocalAgentRuntimeError> {
    let replaced = value.trim().replace('\\', "/");
    let mut segments = Vec::new();
    for segment in replaced.split('/') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if matches!(segment, "." | "..") {
            return Err(LocalAgentRuntimeError::InvalidRequest(
                "notepad folder must not contain dot segments".to_string(),
            ));
        }
        segments.push(segment);
    }
    let folder = segments.join("/");
    if !allow_empty && folder.is_empty() {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "notepad folder is required".to_string(),
        ));
    }
    if folder.len() > 1_000 {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "notepad folder exceeds 1000 bytes".to_string(),
        ));
    }
    Ok(folder)
}

fn normalize_title(title: &str, content: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    content
        .lines()
        .map(str::trim)
        .find_map(|line| {
            line.strip_prefix('#')
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("Untitled")
        .chars()
        .take(1_000)
        .collect()
}

fn normalize_updated_title(title: &str) -> Result<String, LocalAgentRuntimeError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "notepad title must not be empty".to_string(),
        ));
    }
    Ok(title.to_string())
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.to_lowercase()))
        .collect()
}

fn sanitize_image_name(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("image")
        .to_string()
}

fn validate_image_signature(mime_type: &str, data: &[u8]) -> Result<(), LocalAgentRuntimeError> {
    let valid = match mime_type {
        "image/png" => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => data.starts_with(&[0xff, 0xd8, 0xff]),
        "image/gif" => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
        "image/webp" => data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP",
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(LocalAgentRuntimeError::InvalidRequest(
            "notepad image content does not match its MIME type".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CreateNotepadFolderCommand, CreateNotepadNoteCommand, GetNotepadNoteCommand,
        HostRequestEnvelope, ListNotepadFoldersCommand, ListNotepadNotesCommand,
        RenameNotepadFolderCommand, UpdateNotepadNoteCommand, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use std::sync::Arc;

    fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    #[test]
    fn folder_normalization_is_canonical_and_rejects_traversal() {
        assert_eq!(
            normalize_folder(" /work//ideas/ ", false).unwrap(),
            "work/ideas"
        );
        assert!(normalize_folder("work/../private", false).is_err());
    }

    #[test]
    fn tags_are_trimmed_and_case_insensitively_unique() {
        assert_eq!(
            normalize_tags(vec![" Rust ".into(), "rust".into(), "Local".into()]),
            vec!["Rust", "Local"]
        );
    }

    #[tokio::test]
    async fn notepad_lifecycle_is_owner_scoped_and_versioned() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");

        runtime
            .try_handle(request(
                "folder-create-1",
                HostCommand::CreateNotepadFolder(CreateNotepadFolderCommand {
                    owner_user_id: "user-1".to_string(),
                    folder: " work//ideas ".to_string(),
                }),
            ))
            .await
            .expect("create folder");
        let folders = runtime
            .try_handle(request(
                "folder-list-1",
                HostCommand::ListNotepadFolders(ListNotepadFoldersCommand {
                    owner_user_id: "user-1".to_string(),
                }),
            ))
            .await
            .expect("list folders");
        assert!(matches!(
            folders,
            HostResult::NotepadFolders { folders }
                if folders == vec!["".to_string(), "work".to_string(), "work/ideas".to_string()]
        ));

        let created = runtime
            .try_handle(request(
                "note-create-1",
                HostCommand::CreateNotepadNote(CreateNotepadNoteCommand {
                    owner_user_id: "user-1".to_string(),
                    folder: "work/ideas".to_string(),
                    title: String::new(),
                    content: "# Local Host\n\nBody".to_string(),
                    tags: vec![" Rust ".to_string(), "rust".to_string()],
                }),
            ))
            .await
            .expect("create note");
        let HostResult::NotepadNote { detail } = created else {
            panic!("unexpected create result")
        };
        assert_eq!(detail.note.title, "Local Host");
        assert_eq!(detail.note.tags, vec!["Rust"]);
        assert_eq!(detail.note.version, 1);
        let note_id = detail.note.note_id;

        let hidden = runtime
            .handle(request(
                "note-get-other-owner",
                HostCommand::GetNotepadNote(GetNotepadNoteCommand {
                    owner_user_id: "user-2".to_string(),
                    note_id: note_id.clone(),
                }),
            ))
            .await;
        assert_eq!(hidden.error.expect("not found").code, "not_found");

        let updated = runtime
            .try_handle(request(
                "note-update-1",
                HostCommand::UpdateNotepadNote(UpdateNotepadNoteCommand {
                    owner_user_id: "user-1".to_string(),
                    note_id: note_id.clone(),
                    expected_version: 1,
                    title: Some("Updated".to_string()),
                    content: None,
                    folder: None,
                    tags: None,
                }),
            ))
            .await
            .expect("update note");
        assert!(matches!(
            updated,
            HostResult::NotepadNote { detail }
                if detail.note.title == "Updated" && detail.note.version == 2
        ));
        let stale = runtime
            .handle(request(
                "note-update-stale",
                HostCommand::UpdateNotepadNote(UpdateNotepadNoteCommand {
                    owner_user_id: "user-1".to_string(),
                    note_id: note_id.clone(),
                    expected_version: 1,
                    title: Some("Stale".to_string()),
                    content: None,
                    folder: None,
                    tags: None,
                }),
            ))
            .await;
        assert_eq!(stale.error.expect("conflict").code, "conflict");

        runtime
            .try_handle(request(
                "folder-rename-1",
                HostCommand::RenameNotepadFolder(RenameNotepadFolderCommand {
                    owner_user_id: "user-1".to_string(),
                    from: "work".to_string(),
                    to: "archive".to_string(),
                }),
            ))
            .await
            .expect("rename folder");
        let listed = runtime
            .try_handle(request(
                "note-list-1",
                HostCommand::ListNotepadNotes(ListNotepadNotesCommand {
                    owner_user_id: "user-1".to_string(),
                    query: Some("updated".to_string()),
                    limit: 10,
                }),
            ))
            .await
            .expect("list notes");
        assert!(matches!(
            listed,
            HostResult::NotepadNotes { notes }
                if notes.len() == 1 && notes[0].folder == "archive/ideas" && notes[0].version == 3
        ));
    }
}

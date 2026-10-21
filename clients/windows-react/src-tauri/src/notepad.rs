use chatos_local_agent_protocol::{
    CreateNotepadFolderCommand, CreateNotepadNoteCommand, DeleteNotepadFolderCommand,
    DeleteNotepadNoteCommand, GetNotepadNoteCommand, HostCommand, HostResult,
    InitializeNotepadCommand, ListNotepadFoldersCommand, ListNotepadNotesCommand, LocalNotepadNote,
    LocalNotepadNoteDetail, RenameNotepadFolderCommand, UpdateNotepadNoteCommand,
    LOCAL_NOTEPAD_MAX_CONTENT_BYTES, LOCAL_NOTEPAD_MAX_LIST_LIMIT, LOCAL_NOTEPAD_MAX_TAGS,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::{validate_owner, HostManager},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotepadNoteSummary {
    note_id: String,
    title: String,
    folder: String,
    tags: Vec<String>,
    version: u64,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotepadNoteDetail {
    note: NotepadNoteSummary,
    content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotepadSnapshot {
    folders: Vec<String>,
    notes: Vec<NotepadNoteSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadListInput {
    owner_user_id: String,
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadFolderInput {
    owner_user_id: String,
    folder: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadRenameFolderInput {
    owner_user_id: String,
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadDeleteFolderInput {
    owner_user_id: String,
    folder: String,
    recursive: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadCreateNoteInput {
    owner_user_id: String,
    folder: String,
    title: String,
    content: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadNoteIdentityInput {
    owner_user_id: String,
    note_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadUpdateNoteInput {
    owner_user_id: String,
    note_id: String,
    expected_version: u64,
    title: String,
    content: String,
    folder: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadDeleteNoteInput {
    owner_user_id: String,
    note_id: String,
    expected_version: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotepadFolderMutation {
    folder: String,
    affected_notes: u64,
}

#[tauri::command]
pub(crate) async fn load_notepad(
    input: NotepadListInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<NotepadSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let query = validate_query(input.query)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let initialized = manager
        .execute(HostCommand::InitializeNotepad(InitializeNotepadCommand {
            owner_user_id: input.owner_user_id.clone(),
        }))
        .await?;
    let HostResult::NotepadInitialized { .. } = initialized else {
        return Err("Local Agent Host 返回了错误的记事本初始化结果。".into());
    };
    let folders = list_folders(&manager, &input.owner_user_id).await?;
    let notes = list_notes(&manager, &input.owner_user_id, query).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(NotepadSnapshot {
        folders,
        notes: notes.into_iter().map(project_note).collect(),
    })
}

#[tauri::command]
pub(crate) async fn create_notepad_folder(
    input: NotepadFolderInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<NotepadFolderMutation, String> {
    validate_owner(&input.owner_user_id)?;
    let folder = normalize_folder(&input.folder, false)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::CreateNotepadFolder(
            CreateNotepadFolderCommand {
                owner_user_id: input.owner_user_id.clone(),
                folder: folder.clone(),
            },
        ))
        .await?;
    let mutation = project_folder_mutation(result, &folder)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(mutation)
}

#[tauri::command]
pub(crate) async fn rename_notepad_folder(
    input: NotepadRenameFolderInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<NotepadFolderMutation, String> {
    validate_owner(&input.owner_user_id)?;
    let from = normalize_folder(&input.from, false)?;
    let to = normalize_folder(&input.to, false)?;
    if to.starts_with(&format!("{from}/")) {
        return Err("文件夹不能移动到自身内部。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::RenameNotepadFolder(
            RenameNotepadFolderCommand {
                owner_user_id: input.owner_user_id.clone(),
                from,
                to: to.clone(),
            },
        ))
        .await?;
    let mutation = project_folder_mutation(result, &to)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(mutation)
}

#[tauri::command]
pub(crate) async fn delete_notepad_folder(
    input: NotepadDeleteFolderInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<NotepadFolderMutation, String> {
    validate_owner(&input.owner_user_id)?;
    if !input.recursive {
        return Err("当前客户端仅支持确认后的递归删除。".into());
    }
    let folder = normalize_folder(&input.folder, false)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::DeleteNotepadFolder(
            DeleteNotepadFolderCommand {
                owner_user_id: input.owner_user_id.clone(),
                folder: folder.clone(),
                recursive: true,
            },
        ))
        .await?;
    let mutation = project_folder_mutation(result, &folder)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(mutation)
}

#[tauri::command]
pub(crate) async fn create_notepad_note(
    input: NotepadCreateNoteInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<NotepadNoteDetail, String> {
    validate_owner(&input.owner_user_id)?;
    let folder = normalize_folder(&input.folder, true)?;
    validate_note_fields(&input.title, &input.content, &input.tags)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::CreateNotepadNote(CreateNotepadNoteCommand {
            owner_user_id: input.owner_user_id.clone(),
            folder,
            title: input.title,
            content: input.content,
            tags: normalize_tags(input.tags),
        }))
        .await?;
    let detail = require_note_detail(result, &input.owner_user_id, None, None)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(project_detail(detail))
}

#[tauri::command]
pub(crate) async fn get_notepad_note(
    input: NotepadNoteIdentityInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<NotepadNoteDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.note_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let result = manager
        .execute(HostCommand::GetNotepadNote(GetNotepadNoteCommand {
            owner_user_id: input.owner_user_id.clone(),
            note_id: input.note_id.clone(),
        }))
        .await?;
    let detail = require_note_detail(result, &input.owner_user_id, Some(&input.note_id), None)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(project_detail(detail))
}

#[tauri::command]
pub(crate) async fn update_notepad_note(
    input: NotepadUpdateNoteInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<NotepadNoteDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.note_id)?;
    if input.expected_version == 0 {
        return Err("笔记版本无效。".into());
    }
    let folder = normalize_folder(&input.folder, true)?;
    validate_note_fields(&input.title, &input.content, &input.tags)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let current = fetch_note(&manager, &input.owner_user_id, &input.note_id).await?;
    if current.note.version != input.expected_version {
        return Err("笔记已经变化，请刷新后重试。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::UpdateNotepadNote(UpdateNotepadNoteCommand {
            owner_user_id: input.owner_user_id.clone(),
            note_id: input.note_id.clone(),
            expected_version: input.expected_version,
            title: Some(input.title),
            content: Some(input.content),
            folder: Some(folder),
            tags: Some(normalize_tags(input.tags)),
        }))
        .await?;
    let detail = require_note_detail(
        result,
        &input.owner_user_id,
        Some(&input.note_id),
        Some(input.expected_version),
    )?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(project_detail(detail))
}

#[tauri::command]
pub(crate) async fn delete_notepad_note(
    input: NotepadDeleteNoteInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<String, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.note_id)?;
    if input.expected_version == 0 {
        return Err("笔记版本无效。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let current = fetch_note(&manager, &input.owner_user_id, &input.note_id).await?;
    if current.note.version != input.expected_version {
        return Err("笔记已经变化，请刷新后重试。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::DeleteNotepadNote(DeleteNotepadNoteCommand {
            owner_user_id: input.owner_user_id.clone(),
            note_id: input.note_id.clone(),
            expected_version: input.expected_version,
        }))
        .await?;
    let HostResult::NotepadNoteDeleted { note_id } = result else {
        return Err("Local Agent Host 返回了错误的笔记删除结果。".into());
    };
    if note_id != input.note_id {
        return Err("Local Agent Host 返回了不匹配的笔记删除结果。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(note_id)
}

async fn list_folders(manager: &HostManager, owner_user_id: &str) -> Result<Vec<String>, String> {
    let result = manager
        .execute(HostCommand::ListNotepadFolders(ListNotepadFoldersCommand {
            owner_user_id: owner_user_id.to_owned(),
        }))
        .await?;
    let HostResult::NotepadFolders { folders } = result else {
        return Err("Local Agent Host 返回了错误的记事本文件夹列表。".into());
    };
    let mut projected = folders
        .into_iter()
        .map(|folder| normalize_folder(&folder, false))
        .collect::<Result<Vec<_>, _>>()?;
    projected.sort();
    projected.dedup();
    Ok(projected)
}

async fn list_notes(
    manager: &HostManager,
    owner_user_id: &str,
    query: Option<String>,
) -> Result<Vec<LocalNotepadNote>, String> {
    let result = manager
        .execute(HostCommand::ListNotepadNotes(ListNotepadNotesCommand {
            owner_user_id: owner_user_id.to_owned(),
            query,
            limit: LOCAL_NOTEPAD_MAX_LIST_LIMIT,
        }))
        .await?;
    let HostResult::NotepadNotes { notes } = result else {
        return Err("Local Agent Host 返回了错误的笔记列表。".into());
    };
    if notes.iter().any(|note| note.owner_user_id != owner_user_id) {
        return Err("Local Agent Host 返回了不属于当前账号的笔记。".into());
    }
    Ok(notes)
}

async fn fetch_note(
    manager: &HostManager,
    owner_user_id: &str,
    note_id: &str,
) -> Result<LocalNotepadNoteDetail, String> {
    let result = manager
        .execute(HostCommand::GetNotepadNote(GetNotepadNoteCommand {
            owner_user_id: owner_user_id.to_owned(),
            note_id: note_id.to_owned(),
        }))
        .await?;
    require_note_detail(result, owner_user_id, Some(note_id), None)
}

fn require_note_detail(
    result: HostResult,
    owner_user_id: &str,
    note_id: Option<&str>,
    previous_version: Option<u64>,
) -> Result<LocalNotepadNoteDetail, String> {
    let HostResult::NotepadNote { detail } = result else {
        return Err("Local Agent Host 返回了错误的笔记详情。".into());
    };
    if detail.note.owner_user_id != owner_user_id
        || note_id.is_some_and(|expected| detail.note.note_id != expected)
        || detail.note.version == 0
        || previous_version.is_some_and(|version| detail.note.version <= version)
        || detail.content.len() > LOCAL_NOTEPAD_MAX_CONTENT_BYTES
    {
        return Err("Local Agent Host 返回了不匹配的笔记详情。".into());
    }
    Ok(detail)
}

fn project_folder_mutation(
    result: HostResult,
    expected_folder: &str,
) -> Result<NotepadFolderMutation, String> {
    let HostResult::NotepadFolderMutation {
        folder,
        affected_notes,
    } = result
    else {
        return Err("Local Agent Host 返回了错误的文件夹变更结果。".into());
    };
    if folder != expected_folder {
        return Err("Local Agent Host 返回了不匹配的文件夹变更结果。".into());
    }
    Ok(NotepadFolderMutation {
        folder,
        affected_notes,
    })
}

fn project_note(note: LocalNotepadNote) -> NotepadNoteSummary {
    NotepadNoteSummary {
        note_id: note.note_id,
        title: note.title,
        folder: note.folder,
        tags: note.tags,
        version: note.version,
        created_at_unix_ms: note.created_at_unix_ms,
        updated_at_unix_ms: note.updated_at_unix_ms,
    }
}

fn project_detail(detail: LocalNotepadNoteDetail) -> NotepadNoteDetail {
    NotepadNoteDetail {
        note: project_note(detail.note),
        content: detail.content,
    }
}

fn validate_query(query: Option<String>) -> Result<Option<String>, String> {
    let query = query.map(|value| value.trim().to_owned());
    match query {
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if value.len() > 1_000 || value.chars().any(char::is_control) => {
            Err("记事本搜索内容无效。".into())
        }
        value => Ok(value),
    }
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        return Err("笔记标识无效。".into());
    }
    Ok(())
}

fn normalize_folder(value: &str, allow_empty: bool) -> Result<String, String> {
    if value.contains('\0') || value.len() > 1_000 {
        return Err("记事本文件夹无效。".into());
    }
    let replaced = value.trim().replace('\\', "/");
    let mut parts = Vec::new();
    for part in replaced.split('/') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if matches!(part, "." | "..") || part.chars().any(char::is_control) {
            return Err("记事本文件夹无效。".into());
        }
        parts.push(part);
    }
    let normalized = parts.join("/");
    if normalized.is_empty() && !allow_empty {
        return Err("记事本文件夹不能为空。".into());
    }
    Ok(normalized)
}

fn validate_note_fields(title: &str, content: &str, tags: &[String]) -> Result<(), String> {
    if title.len() > 1_000 || title.contains('\0') || title.chars().any(|value| value == '\r') {
        return Err("笔记标题无效。".into());
    }
    if content.len() > LOCAL_NOTEPAD_MAX_CONTENT_BYTES || content.contains('\0') {
        return Err(format!(
            "笔记正文不能超过 {} KiB。",
            LOCAL_NOTEPAD_MAX_CONTENT_BYTES / 1_024
        ));
    }
    if tags.len() > LOCAL_NOTEPAD_MAX_TAGS
        || tags.iter().any(|tag| {
            tag.trim().is_empty()
                || tag.trim() != tag
                || tag.len() > 128
                || tag.chars().any(char::is_control)
        })
    {
        return Err("笔记标签无效。".into());
    }
    Ok(())
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for tag in tags {
        if !result.contains(&tag) {
            result.push(tag);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> LocalNotepadNote {
        LocalNotepadNote {
            note_id: "note-1".into(),
            owner_user_id: "owner-1".into(),
            title: "方案".into(),
            folder: "工作/方案".into(),
            tags: vec!["重要".into()],
            file: "/private/notepad/note-1.md".into(),
            version: 3,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
        }
    }

    #[test]
    fn public_projection_does_not_expose_owner_or_file_path() {
        let projected = project_note(note());
        let json = serde_json::to_value(projected).unwrap();

        assert_eq!(json["noteId"], "note-1");
        assert_eq!(json["version"], 3);
        assert!(json.get("ownerUserId").is_none());
        assert!(json.get("file").is_none());
    }

    #[test]
    fn folders_are_normalized_and_dot_segments_are_rejected() {
        assert_eq!(
            normalize_folder(" 工作\\方案/ ", false).unwrap(),
            "工作/方案"
        );
        assert!(normalize_folder("工作/../秘密", false).is_err());
        assert!(normalize_folder("  ", false).is_err());
        assert_eq!(normalize_folder("  ", true).unwrap(), "");
    }

    #[test]
    fn detail_validation_checks_identity_version_and_size() {
        let detail = LocalNotepadNoteDetail {
            note: note(),
            content: "body".into(),
        };
        assert!(require_note_detail(
            HostResult::NotepadNote {
                detail: detail.clone()
            },
            "owner-1",
            Some("note-1"),
            Some(2),
        )
        .is_ok());
        assert!(require_note_detail(
            HostResult::NotepadNote { detail },
            "owner-2",
            Some("note-1"),
            None,
        )
        .is_err());
    }
}

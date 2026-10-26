use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_local_agent_protocol::{
    CreateNotepadFolderCommand, CreateNotepadNoteCommand, DeleteNotepadFolderCommand,
    DeleteNotepadNoteCommand, GetNotepadNoteCommand, HostCommand, HostResult,
    InitializeNotepadCommand, ListNotepadFoldersCommand, ListNotepadNotesCommand, LocalNotepadNote,
    LocalNotepadNoteDetail, PutNotepadImageCommand, RenameNotepadFolderCommand,
    UpdateNotepadNoteCommand, LOCAL_NOTEPAD_MAX_CONTENT_BYTES, LOCAL_NOTEPAD_MAX_IMAGE_BYTES,
    LOCAL_NOTEPAD_MAX_LIST_LIMIT, LOCAL_NOTEPAD_MAX_TAGS,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Read},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;

use crate::{
    app_owned_export,
    authentication::AuthenticationManager,
    host_lifecycle::{validate_owner, HostManager},
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePriority,
    },
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NotepadImageSource {
    Picker,
    Clipboard,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadImageInput {
    owner_user_id: String,
    note_id: String,
    expected_version: u64,
    source: NotepadImageSource,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadCopyTextInput {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotepadExportMarkdownInput {
    owner_user_id: String,
    title: String,
    content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotepadImageInsertion {
    note_id: String,
    expected_version: u64,
    name: String,
    mime_type: String,
    size: u64,
    sha256: String,
    markdown: String,
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
pub(crate) async fn copy_notepad_text(input: NotepadCopyTextInput) -> Result<(), String> {
    if input.text.len() > LOCAL_NOTEPAD_MAX_CONTENT_BYTES || input.text.contains('\0') {
        return Err("复制的笔记正文无效。".into());
    }
    tokio::task::spawn_blocking(move || {
        let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问剪贴板。")?;
        clipboard
            .set_text(input.text)
            .map_err(|_| "无法复制笔记正文。".to_owned())
    })
    .await
    .map_err(|error| format!("无法复制笔记正文：{error}"))?
}

#[tauri::command]
pub(crate) async fn export_notepad_markdown(
    input: NotepadExportMarkdownInput,
    authentication: State<'_, AuthenticationManager>,
    resources: State<'_, MachineResourceGovernor>,
) -> Result<bool, String> {
    validate_owner(&input.owner_user_id)?;
    validate_note_fields(&input.title, &input.content, &[])?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title("导出 Markdown")
        .add_filter("Markdown", &["md"])
        .set_file_name(export_markdown_file_name(&input.title))
        .save_file()
        .await;
    let Some(selected) = selected else {
        return Ok(false);
    };
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    let path = selected.path().to_path_buf();
    let content = input.content;
    tokio::task::spawn_blocking(move || {
        app_owned_export::export_bytes(&path, ".chatos-notepad-export-", content.as_bytes())
    })
    .await
    .map_err(|error| format!("无法导出 Markdown：{error}"))??;
    Ok(true)
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

#[tauri::command]
pub(crate) async fn import_notepad_image(
    input: NotepadImageInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<Option<NotepadImageInsertion>, String> {
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
    let candidate =
        match input.source {
            NotepadImageSource::Picker => {
                let selected = rfd::AsyncFileDialog::new()
                    .set_title("选择笔记图片")
                    .add_filter("图片", &["png", "jpg", "jpeg", "webp", "gif"])
                    .pick_file()
                    .await;
                let Some(selected) = selected else {
                    return Ok(None);
                };
                let path = selected.path().to_path_buf();
                tokio::task::spawn_blocking(move || read_image_file(&path))
                    .await
                    .map_err(|error| format!("无法读取图片：{error}"))??
            }
            NotepadImageSource::Clipboard => tokio::task::spawn_blocking(read_clipboard_image)
                .await
                .map_err(|error| format!("无法读取剪贴板图片：{error}"))??,
        };
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let data_base64 = STANDARD.encode(&candidate.bytes);
    let projected_markdown_bytes = current.content.len()
        + candidate.name.len()
        + candidate.mime_type.len()
        + data_base64.len()
        + 32;
    if projected_markdown_bytes > LOCAL_NOTEPAD_MAX_CONTENT_BYTES {
        return Err("图片插入后会超过 512 KiB 的笔记正文限制，请压缩图片或新建笔记。".into());
    }
    let expected_sha256 = format!("{:x}", Sha256::digest(&candidate.bytes));
    let expected_size = u64::try_from(candidate.bytes.len()).map_err(|_| "图片大小无效。")?;
    let result = manager
        .execute(HostCommand::PutNotepadImage(PutNotepadImageCommand {
            owner_user_id: input.owner_user_id.clone(),
            note_id: input.note_id.clone(),
            name: candidate.name.clone(),
            mime_type: candidate.mime_type.clone(),
            data_base64,
        }))
        .await?;
    let HostResult::NotepadImage { image } = result else {
        return Err("Local Agent Host 返回了错误的笔记图片结果。".into());
    };
    let expected_data_url_prefix = format!("data:{};base64,", candidate.mime_type);
    if image.owner_user_id != input.owner_user_id
        || image.note_id != input.note_id
        || image.name != candidate.name
        || image.mime_type != candidate.mime_type
        || image.size != expected_size
        || image.sha256 != expected_sha256
        || !image.data_url.starts_with(&expected_data_url_prefix)
        || image.data_url.len()
            > expected_data_url_prefix.len() + candidate.bytes.len().div_ceil(3) * 4
    {
        return Err("Local Agent Host 返回了不匹配的笔记图片结果。".into());
    }
    let decoded = STANDARD
        .decode(&image.data_url[expected_data_url_prefix.len()..])
        .map_err(|_| "Local Agent Host 返回了无效的笔记图片。")?;
    if decoded != candidate.bytes {
        return Err("Local Agent Host 返回了不匹配的笔记图片内容。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let alt = markdown_alt(&image.name);
    Ok(Some(NotepadImageInsertion {
        note_id: image.note_id,
        expected_version: input.expected_version,
        name: image.name,
        mime_type: image.mime_type,
        size: image.size,
        sha256: image.sha256,
        markdown: format!("![{alt}](<{}>)", image.data_url),
    }))
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
    // The Host includes the implicit root folder as an empty string.  The
    // macOS client treats that root as a valid note location but does not
    // expose it as a child folder, so keep it out of the projected folder
    // list instead of rejecting an otherwise empty notebook on every load.
    project_folders(folders)
}

fn project_folders(folders: Vec<String>) -> Result<Vec<String>, String> {
    let mut projected = folders
        .into_iter()
        .map(|folder| normalize_folder(&folder, true))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|folder| !folder.is_empty())
        .collect::<Vec<_>>();
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

fn export_markdown_file_name(title: &str) -> String {
    let cleaned = title
        .trim()
        .chars()
        .filter(|value| {
            !value.is_control()
                && !matches!(value, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
        .collect::<String>();
    let cleaned = cleaned.trim_end_matches([' ', '.']);
    let base = if cleaned.is_empty() {
        "未命名笔记"
    } else {
        cleaned
    };
    format!("{base}.md")
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

struct NotepadImageCandidate {
    name: String,
    mime_type: String,
    bytes: Vec<u8>,
}

fn read_image_file(path: &Path) -> Result<NotepadImageCandidate, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_image_name)
        .unwrap_or_else(|| "笔记图片".into());
    let before = fs::symlink_metadata(path).map_err(|_| format!("无法读取“{name}”。"))?;
    if !before.file_type().is_file() || before.file_type().is_symlink() {
        return Err(format!("“{name}”不是可导入的图片文件。"));
    }
    if before.len() == 0 || before.len() > LOCAL_NOTEPAD_MAX_IMAGE_BYTES as u64 {
        return Err(format!("“{name}”必须小于 512 KiB。"));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options
        .open(path)
        .map_err(|_| format!("无法读取“{name}”。"))?;
    let opened = file.metadata().map_err(|_| format!("无法读取“{name}”。"))?;
    if !opened.is_file() || opened.len() != before.len() {
        return Err(format!("“{name}”在读取前发生了变化，请重新选择。"));
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.by_ref()
        .take((LOCAL_NOTEPAD_MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("无法读取“{name}”。"))?;
    let after = file.metadata().map_err(|_| format!("无法读取“{name}”。"))?;
    if bytes.len() != opened.len() as usize
        || bytes.len() > LOCAL_NOTEPAD_MAX_IMAGE_BYTES
        || after.len() != opened.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(format!("“{name}”在读取期间发生了变化，请重新选择。"));
    }
    let mime_type = image_mime_type(&bytes)?;
    Ok(NotepadImageCandidate {
        name,
        mime_type: mime_type.into(),
        bytes,
    })
}

fn read_clipboard_image() -> Result<NotepadImageCandidate, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问剪贴板。")?;
    let image = clipboard
        .get_image()
        .map_err(|_| "剪贴板中没有可导入的图片。")?;
    let expected = image
        .width
        .checked_mul(image.height)
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| "剪贴板图片尺寸无效。".to_owned())?;
    if expected != image.bytes.len() || expected > 64 * 1_024 * 1_024 {
        return Err("剪贴板图片过大或格式无效。".into());
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            Cursor::new(&mut bytes),
            image.width as u32,
            image.height as u32,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|_| "无法处理剪贴板图片。")?;
        writer
            .write_image_data(image.bytes.as_ref())
            .map_err(|_| "无法处理剪贴板图片。")?;
    }
    if bytes.is_empty() || bytes.len() > LOCAL_NOTEPAD_MAX_IMAGE_BYTES {
        return Err("剪贴板图片压缩后超过 512 KiB，请缩小后重试。".into());
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(NotepadImageCandidate {
        name: format!("粘贴的图片-{timestamp}.png"),
        mime_type: "image/png".into(),
        bytes,
    })
}

fn image_mime_type(bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Ok("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Ok("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Ok("image/webp")
    } else {
        Err("仅支持 PNG、JPEG、WebP 或 GIF 图片。".into())
    }
}

fn sanitize_image_name(value: &str) -> String {
    let name: String = value
        .chars()
        .filter(|value| !value.is_control() && !matches!(value, '/' | '\\'))
        .take(512)
        .collect();
    if name.trim().is_empty() {
        "笔记图片".into()
    } else {
        name
    }
}

fn markdown_alt(value: &str) -> String {
    let value = Path::new(value)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(value);
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
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
    fn root_folder_is_not_projected_as_a_child_folder() {
        let projected =
            project_folders(vec!["".into(), " 工作\\方案/ ".into(), "工作/方案".into()]).unwrap();
        assert_eq!(projected, vec!["工作/方案"]);
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

    #[test]
    fn image_projection_accepts_only_supported_signatures_and_escapes_alt_text() {
        assert_eq!(
            image_mime_type(b"\x89PNG\r\n\x1a\nrest").unwrap(),
            "image/png"
        );
        assert!(image_mime_type(b"<svg></svg>").is_err());
        assert_eq!(sanitize_image_name("../screen[1].png"), "..screen[1].png");
        assert_eq!(markdown_alt("screen[1].png"), "screen\\[1\\]");
    }

    #[test]
    fn export_file_name_is_safe_and_keeps_markdown_extension() {
        assert_eq!(export_markdown_file_name(" 方案/评审:*? "), "方案评审.md");
        assert_eq!(export_markdown_file_name("..."), "未命名笔记.md");
    }
}

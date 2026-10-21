use super::team_todo::todo_planning_reference;
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) const MAXIMUM_DOCUMENTS_PER_MESSAGE: usize = 5;
const MAXIMUM_DOCUMENTS_PER_RUN: i64 = 20;
const MAXIMUM_DOCUMENT_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_DOCUMENT_BYTES_PER_RUN: i64 = 8 * 1_024 * 1_024;
const MAXIMUM_ATTACHMENT_READ_CHARACTERS: usize = 12_000;

#[derive(Clone)]
pub(super) struct AgentDocumentDraft {
    pub(super) reference: String,
    pub(super) name: String,
    pub(super) size: usize,
    pub(super) sha256: String,
    pub(super) relative_path: String,
    pub(super) data: Vec<u8>,
}

impl AgentConversationStore {
    pub(crate) fn create_agent_chat_document(
        &self,
        owner_user_id: &str,
        run_id: &str,
        request_key: &str,
        raw_name: &str,
        title: &str,
        markdown: &str,
        now_unix_ms: i64,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| document_error())?;
        validate_text(request_key, 512, false).map_err(|_| document_error())?;
        let name = sanitized_markdown_document_name(raw_name).ok_or_else(document_error)?;
        validate_text(title, 512, false).map_err(|_| document_error())?;
        if title.chars().any(char::is_control) || markdown.trim().is_empty() {
            return Err(document_error());
        }
        let data = markdown.as_bytes();
        if data.len() > MAXIMUM_DOCUMENT_BYTES || now_unix_ms < 0 {
            return Err(document_error());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| document_store_error())?;
        let binding = load_agent_communication_tool_binding(&transaction, owner_user_id, run_id)?;
        let existing = transaction
            .query_row(
                "SELECT document_ref, name, title, size_bytes, sha256 \
                 FROM windows_agent_document_drafts \
                 WHERE owner_user_id = ?1 AND run_id = ?2 AND request_key = ?3",
                params![owner_user_id, run_id, request_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| document_store_error())?;
        let sha256 = format!("{:x}", Sha256::digest(data));
        if let Some((reference, stored_name, stored_title, size, stored_sha256)) = existing {
            if stored_name != name
                || stored_title != title
                || size != data.len() as i64
                || stored_sha256 != sha256
            {
                return Err(
                    "The tool invocation ID was reused with different document input.".into(),
                );
            }
            transaction.commit().map_err(|_| document_store_error())?;
            return Ok((
                binding,
                document_response(&reference, &name, title, data.len(), &sha256),
            ));
        }
        let (count, total_bytes): (i64, i64) = transaction
            .query_row(
                "SELECT count(*), COALESCE(sum(size_bytes), 0) \
                 FROM windows_agent_document_drafts WHERE owner_user_id = ?1 AND run_id = ?2",
                params![owner_user_id, run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| document_store_error())?;
        if count >= MAXIMUM_DOCUMENTS_PER_RUN
            || total_bytes.saturating_add(data.len() as i64) > MAXIMUM_DOCUMENT_BYTES_PER_RUN
        {
            return Err("The current Run document limit has been reached.".into());
        }
        let draft_id = identifier("document-draft");
        let reference = todo_planning_reference(owner_user_id, run_id, "document", &draft_id);
        let relative_path = format!("drafts/{run_id}/{draft_id}");
        let path = attachment_path(&self.database, &relative_path, true)?;
        write_private_file(&path, data)?;
        let inserted = transaction.execute(
            r#"
            INSERT INTO windows_agent_document_drafts (
                owner_user_id, run_id, request_key, document_ref, name, title,
                size_bytes, sha256, relative_path, status, created_at_unix_ms,
                consumed_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'available', ?10, NULL)
            "#,
            params![
                owner_user_id,
                run_id,
                request_key,
                reference,
                name,
                title,
                data.len() as i64,
                sha256,
                relative_path,
                now_unix_ms,
            ],
        );
        if inserted.is_err() || transaction.commit().is_err() {
            let _ = fs::remove_file(path);
            return Err(document_store_error());
        }
        Ok((
            binding,
            document_response(&reference, &name, title, data.len(), &sha256),
        ))
    }

    pub(crate) fn read_agent_chat_attachment(
        &self,
        owner_user_id: &str,
        run_id: &str,
        message_reference: &str,
        attachment_reference: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        if limit == 0 || limit > MAXIMUM_ATTACHMENT_READ_CHARACTERS {
            return Err(document_error());
        }
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_communication_tool_binding(&connection, owner_user_id, run_id)?;
        let record = connection
            .query_row(
                r#"
                SELECT attachment.name, attachment.mime_type, attachment.size_bytes,
                       attachment.kind, attachment.relative_path, attachment.sha256
                FROM windows_agent_chat_message_references message_ref
                JOIN windows_agent_chat_attachment_references attachment_ref
                  ON attachment_ref.owner_user_id = message_ref.owner_user_id
                 AND attachment_ref.run_id = message_ref.run_id
                 AND attachment_ref.room_id = message_ref.room_id
                 AND attachment_ref.message_id = message_ref.message_id
                JOIN project_agent_message_attachments attachment
                  ON attachment.owner_user_id = attachment_ref.owner_user_id
                 AND attachment.message_id = attachment_ref.message_id
                 AND attachment.id = attachment_ref.attachment_id
                JOIN project_agent_room_members member
                  ON member.owner_user_id = attachment_ref.owner_user_id
                 AND member.room_id = attachment_ref.room_id AND member.agent_id = ?5
                WHERE message_ref.owner_user_id = ?1 AND message_ref.run_id = ?2
                  AND message_ref.message_ref = ?3
                  AND attachment_ref.attachment_ref = ?4 AND member.status = 'active'
                "#,
                params![
                    owner_user_id,
                    run_id,
                    message_reference,
                    attachment_reference,
                    binding.agent_id,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| document_store_error())?
            .ok_or_else(|| "The attachment reference is unavailable for this Run.".to_owned())?;
        let (name, mime_type, size, kind, relative_path, sha256) = record;
        if size <= 0 || size as usize > MAXIMUM_DOCUMENT_BYTES {
            return Err(document_store_error());
        }
        let path = attachment_path(&self.database, &relative_path, false)?;
        let data = read_verified_file(&path, size as usize, &sha256)?;
        let text = String::from_utf8(data).map_err(|_| {
            "This binary attachment cannot be read as text in the Windows Agent runtime.".to_owned()
        })?;
        let characters = text.chars().collect::<Vec<_>>();
        let start = offset.min(characters.len());
        let end = start.saturating_add(limit).min(characters.len());
        Ok((
            binding,
            json!({
                "message_ref": message_reference,
                "attachment_ref": attachment_reference,
                "name": name,
                "mime_type": mime_type,
                "kind": kind,
                "size": size,
                "content": characters[start..end].iter().collect::<String>(),
                "offset": start,
                "next_offset": if end < characters.len() { Some(end) } else { None },
                "has_more": end < characters.len(),
            }),
        ))
    }
}

pub(super) fn load_document_drafts(
    connection: &Connection,
    database: &Path,
    owner_user_id: &str,
    run_id: &str,
    references: &[String],
) -> Result<Vec<AgentDocumentDraft>, String> {
    if references.len() > MAXIMUM_DOCUMENTS_PER_MESSAGE {
        return Err("Each message may attach at most five documents.".into());
    }
    let mut unique = std::collections::HashSet::new();
    let mut drafts = Vec::with_capacity(references.len());
    for reference in references {
        if !unique.insert(reference) {
            return Err("Document references must be unique.".into());
        }
        let draft = connection
            .query_row(
                "SELECT name, size_bytes, sha256, relative_path \
                 FROM windows_agent_document_drafts WHERE owner_user_id = ?1 AND run_id = ?2 \
                 AND document_ref = ?3 AND status = 'available'",
                params![owner_user_id, run_id, reference],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| document_store_error())?
            .ok_or_else(|| "The document reference is unavailable for this Run.".to_owned())?;
        let (name, size, sha256, relative_path) = draft;
        if size <= 0 || size as usize > MAXIMUM_DOCUMENT_BYTES {
            return Err(document_store_error());
        }
        let path = attachment_path(database, &relative_path, false)?;
        let data = read_verified_file(&path, size as usize, &sha256)?;
        drafts.push(AgentDocumentDraft {
            reference: reference.clone(),
            name,
            size: size as usize,
            sha256,
            relative_path,
            data,
        });
    }
    Ok(drafts)
}

pub(super) fn persist_document_attachments(
    connection: &Connection,
    database: &Path,
    owner_user_id: &str,
    run_id: &str,
    message_id: &str,
    drafts: &[AgentDocumentDraft],
    now_unix_ms: i64,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for (position, draft) in drafts.iter().enumerate() {
        let attachment_id = identifier("attachment");
        let relative_path = format!("messages/{message_id}/{attachment_id}");
        let path = attachment_path(database, &relative_path, true)?;
        if let Err(error) = write_private_file(&path, &draft.data) {
            remove_files(&paths);
            return Err(error);
        }
        paths.push(path);
        if connection
            .execute(
                r#"
                INSERT INTO project_agent_message_attachments (
                    owner_user_id, message_id, id, position, name, mime_type,
                    size_bytes, kind, origin, relative_path, sha256, sync_status,
                    source_document_ref
                ) VALUES (?1, ?2, ?3, ?4, ?5, 'text/markdown; charset=utf-8',
                          ?6, 'file', 'file', ?7, ?8, 'local_only', ?9)
                "#,
                params![
                    owner_user_id,
                    message_id,
                    attachment_id,
                    position as i64,
                    draft.name,
                    draft.size as i64,
                    relative_path,
                    draft.sha256,
                    draft.reference,
                ],
            )
            .is_err()
        {
            remove_files(&paths);
            return Err(document_store_error());
        }
        let changed = match connection.execute(
            "UPDATE windows_agent_document_drafts SET status = 'consumed', \
                 consumed_at_unix_ms = ?1 WHERE owner_user_id = ?2 AND run_id = ?3 \
                 AND document_ref = ?4 AND status = 'available'",
            params![now_unix_ms, owner_user_id, run_id, draft.reference],
        ) {
            Ok(changed) => changed,
            Err(_) => {
                remove_files(&paths);
                return Err(document_store_error());
            }
        };
        if changed != 1 {
            remove_files(&paths);
            return Err("The document reference was consumed concurrently.".into());
        }
    }
    Ok(paths)
}

pub(super) fn remove_files(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn remove_consumed_draft_files(database: &Path, drafts: &[AgentDocumentDraft]) {
    for draft in drafts {
        if let Ok(path) = attachment_path(database, &draft.relative_path, false) {
            let _ = fs::remove_file(path);
        }
    }
}

pub(super) fn existing_document_references(
    connection: &Connection,
    owner_user_id: &str,
    message_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT source_document_ref FROM project_agent_message_attachments \
             WHERE owner_user_id = ?1 AND message_id = ?2 ORDER BY position",
        )
        .map_err(|_| document_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, message_id], |row| row.get(0))
        .map_err(|_| document_store_error())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| document_store_error())
}

pub(super) fn attachment_summaries_for_messages(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    messages: &[(String, String)],
    now_unix_ms: i64,
) -> Result<std::collections::HashMap<String, Vec<Value>>, String> {
    if messages.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = std::iter::repeat_n("?", messages.len())
        .collect::<Vec<_>>()
        .join(", ");
    let mut statement = connection
        .prepare(&format!(
            "SELECT message_id, id, name, mime_type, size_bytes, kind \
             FROM project_agent_message_attachments WHERE owner_user_id = ? \
             AND message_id IN ({placeholders}) ORDER BY message_id, position"
        ))
        .map_err(|_| document_store_error())?;
    let parameters = std::iter::once(owner_user_id.to_owned())
        .chain(messages.iter().map(|(_, message_id)| message_id.clone()))
        .collect::<Vec<_>>();
    let rows = statement
        .query_map(rusqlite::params_from_iter(parameters), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|_| document_store_error())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| document_store_error())?;
    let rooms = messages
        .iter()
        .map(|(room_id, message_id)| (message_id.as_str(), room_id.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut summaries = std::collections::HashMap::<String, Vec<Value>>::new();
    for (message_id, attachment_id, name, mime_type, size, kind) in rows {
        let room_id = rooms
            .get(message_id.as_str())
            .ok_or_else(document_store_error)?;
        let reference =
            todo_planning_reference(owner_user_id, run_id, "attachment", &attachment_id);
        connection
            .execute(
                r#"
                INSERT OR IGNORE INTO windows_agent_chat_attachment_references (
                    owner_user_id, run_id, attachment_ref, room_id, message_id,
                    attachment_id, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    owner_user_id,
                    run_id,
                    reference,
                    *room_id,
                    message_id,
                    attachment_id,
                    now_unix_ms,
                ],
            )
            .map_err(|_| document_store_error())?;
        let issued_authority = connection
            .query_row(
                "SELECT room_id, message_id, attachment_id \
                 FROM windows_agent_chat_attachment_references \
                 WHERE owner_user_id = ?1 AND run_id = ?2 AND attachment_ref = ?3",
                params![owner_user_id, run_id, reference],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(|_| document_store_error())?;
        if issued_authority
            != (
                (*room_id).to_owned(),
                message_id.clone(),
                attachment_id.clone(),
            )
        {
            return Err(document_store_error());
        }
        summaries.entry(message_id).or_default().push(json!({
            "attachment_ref": reference,
            "name": name,
            "mime_type": mime_type,
            "size": size,
            "kind": kind,
        }));
    }
    Ok(summaries)
}

fn document_response(reference: &str, name: &str, title: &str, size: usize, sha256: &str) -> Value {
    json!({
        "document_ref": reference,
        "name": name,
        "title": title,
        "size": size,
        "mime_type": "text/markdown",
        "sha256": sha256,
        "instruction": "请在下一次发送消息时通过 document_refs 附加该文档。",
    })
}

fn sanitized_markdown_document_name(raw_value: &str) -> Option<String> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut value = trimmed
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '/' | '\\' | ':') {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    while value.contains("..") {
        value = value.replace("..", ".");
    }
    value = value
        .trim_matches(|character: char| character.is_whitespace() || matches!(character, '.' | '-'))
        .to_owned();
    if value.is_empty() {
        return None;
    }
    if !value.to_lowercase().ends_with(".md") {
        value.push_str(".md");
    }
    if value.chars().count() > 240 {
        value = format!(
            "{}.md",
            value
                .trim_end_matches(".md")
                .chars()
                .take(237)
                .collect::<String>()
        );
    }
    Some(value)
}

fn attachment_root(database: &Path) -> Result<PathBuf, String> {
    let parent = database.parent().ok_or_else(document_store_error)?;
    let root = parent.join("AgentMessageAttachments");
    ensure_private_directory(&root)?;
    Ok(root)
}

fn attachment_path(
    database: &Path,
    relative_path: &str,
    create_parent: bool,
) -> Result<PathBuf, String> {
    if relative_path.is_empty()
        || relative_path.starts_with('/')
        || relative_path.contains(':')
        || relative_path.split('/').any(|component| {
            component.is_empty()
                || component == "."
                || component == ".."
                || component.contains('\\')
        })
    {
        return Err(document_store_error());
    }
    let root = attachment_root(database)?;
    let path = root.join(relative_path);
    if create_parent {
        let parent = path.parent().ok_or_else(document_store_error)?;
        ensure_private_directory(parent)?;
    }
    let canonical_root = fs::canonicalize(&root).map_err(|_| document_store_error())?;
    let parent = path.parent().ok_or_else(document_store_error)?;
    let canonical_parent = fs::canonicalize(parent).map_err(|_| document_store_error())?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(document_store_error());
    }
    Ok(path)
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(document_store_error());
        }
        return set_private_permissions(path, true);
    }
    let parent = path.parent().ok_or_else(document_store_error)?;
    if parent.exists() {
        let metadata = fs::symlink_metadata(parent).map_err(|_| document_store_error())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(document_store_error());
        }
    } else {
        ensure_private_directory(parent)?;
    }
    fs::create_dir(path).map_err(|_| document_store_error())?;
    set_private_permissions(path, true)
}

fn write_private_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| document_store_error())?;
    if file.write_all(data).and_then(|_| file.sync_all()).is_err() {
        let _ = fs::remove_file(path);
        return Err(document_store_error());
    }
    Ok(())
}

fn read_verified_file(
    path: &Path,
    expected_size: usize,
    expected_sha256: &str,
) -> Result<Vec<u8>, String> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| document_store_error())?;
    if link_metadata.file_type().is_symlink()
        || !link_metadata.is_file()
        || link_metadata.len() != expected_size as u64
    {
        return Err(document_store_error());
    }
    let mut options = fs::OpenOptions::new();
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
    let mut file = options.open(path).map_err(|_| document_store_error())?;
    let before = file.metadata().map_err(|_| document_store_error())?;
    if !before.is_file() || before.len() != expected_size as u64 {
        return Err(document_store_error());
    }
    let mut data = Vec::with_capacity(expected_size);
    (&mut file)
        .take(expected_size.saturating_add(1) as u64)
        .read_to_end(&mut data)
        .map_err(|_| document_store_error())?;
    let after = file.metadata().map_err(|_| document_store_error())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || data.len() != expected_size
        || format!("{:x}", Sha256::digest(&data)) != expected_sha256
    {
        return Err("The Agent document integrity check failed.".into());
    }
    Ok(data)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path, directory: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )
    .map_err(|_| document_store_error())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path, _directory: bool) -> Result<(), String> {
    Ok(())
}

fn document_error() -> String {
    "The Agent document input is invalid.".into()
}

fn document_store_error() -> String {
    "The Agent document store is unavailable.".into()
}

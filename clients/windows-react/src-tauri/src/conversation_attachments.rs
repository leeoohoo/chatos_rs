use chatos_local_agent_protocol::{
    GetConversationAttachmentCommand, HostCommand, HostResult, LocalConversationAttachmentRecord,
    LocalConversationAttachmentSpec,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{http, State};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::host_lifecycle::HostManager;

const MAXIMUM_ATTACHMENT_COUNT: usize = 20;
const MAXIMUM_ATTACHMENT_BYTES: usize = 20 * 1_024 * 1_024;
const LONG_PASTE_CHARACTER_THRESHOLD: usize = 4_000;
const LONG_PASTE_BYTE_THRESHOLD: usize = 8_000;
const MAXIMUM_TEXT_PREVIEW_BYTES: usize = 16 * 1_024;
const REFERENCE_PREFIX: &str = "local-attachment:";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentScope {
    owner_user_id: String,
    conversation_id: String,
    #[serde(default)]
    draft_generation: u64,
}

impl AttachmentScope {
    pub fn new(owner_user_id: &str, conversation_id: &str) -> Self {
        Self {
            owner_user_id: owner_user_id.to_owned(),
            conversation_id: conversation_id.to_owned(),
            draft_generation: 0,
        }
    }

    fn validated(self) -> Result<Self, String> {
        let owner_user_id = required("ownerUserId", self.owner_user_id)?;
        let conversation_id = required("conversationId", self.conversation_id)?;
        Ok(Self {
            owner_user_id,
            conversation_id,
            draft_generation: self.draft_generation,
        })
    }

    fn key(&self) -> ScopeKey {
        ScopeKey {
            owner_hash: digest(self.owner_user_id.as_bytes()),
            conversation_hash: digest(self.conversation_id.as_bytes()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ScopeKey {
    owner_hash: String,
    conversation_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AttachmentKind {
    Image,
    File,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AttachmentOrigin {
    File,
    PastedImage,
    PastedText,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAttachmentDraft {
    id: String,
    name: String,
    mime_type: String,
    kind: AttachmentKind,
    origin: AttachmentOrigin,
    byte_size: u64,
    preview_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentImportResult {
    scope: AttachmentScope,
    attachments: Vec<ConversationAttachmentDraft>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDragState {
    pub scope: Option<AttachmentScope>,
    pub targeted: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardImportResult {
    kind: &'static str,
    text: Option<String>,
    result: Option<AttachmentImportResult>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentTextPreview {
    content: String,
    truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentIDsInput {
    #[serde(flatten)]
    scope: AttachmentScope,
    attachment_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentPreviewInput {
    #[serde(flatten)]
    scope: AttachmentScope,
    attachment_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAttachmentsInput {
    #[serde(flatten)]
    scope: AttachmentScope,
    authorized_local_refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAttachmentPreviewInput {
    #[serde(flatten)]
    scope: AttachmentScope,
    authorized_local_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultMetadata {
    attachment_id: String,
    token: String,
    display_name: String,
    media_type: String,
    byte_size: u64,
    sha256: String,
    kind: AttachmentKind,
    origin: AttachmentOrigin,
}

impl VaultMetadata {
    fn draft(&self, scope: &ScopeKey) -> ConversationAttachmentDraft {
        ConversationAttachmentDraft {
            id: self.attachment_id.clone(),
            name: self.display_name.clone(),
            mime_type: self.media_type.clone(),
            kind: self.kind.clone(),
            origin: self.origin.clone(),
            byte_size: self.byte_size,
            preview_url: safe_preview_media_type(&self.media_type)
                .then(|| preview_url(scope, &self.token)),
        }
    }

    fn spec(&self) -> LocalConversationAttachmentSpec {
        LocalConversationAttachmentSpec {
            attachment_id: self.attachment_id.clone(),
            display_name: self.display_name.clone(),
            media_type: self.media_type.clone(),
            byte_size: self.byte_size,
            sha256: self.sha256.clone(),
            authorized_local_ref: format!("{REFERENCE_PREFIX}{}", self.token),
            metadata: json!({"kind": self.kind}),
        }
    }
}

#[derive(Default)]
struct AttachmentState {
    active_scope: Option<AttachmentScope>,
    drafts: HashMap<(ScopeKey, String), VaultMetadata>,
}

struct AttachmentManagerInner {
    root: PathBuf,
    state: Mutex<AttachmentState>,
    import_lock: AsyncMutex<()>,
}

#[derive(Clone)]
pub struct AttachmentManager {
    inner: Arc<AttachmentManagerInner>,
}

impl AttachmentManager {
    pub fn from_runtime() -> Result<Self, String> {
        let root = dirs::data_local_dir()
            .ok_or_else(|| "无法定位应用数据目录。".to_string())?
            .join("com.chatos.windows")
            .join("attachments");
        Self::new(root)
    }

    pub fn new(root: PathBuf) -> Result<Self, String> {
        if let Some(parent) = root.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("无法创建应用数据目录：{error}"))?;
        }
        ensure_private_directory(&root)?;
        let root =
            fs::canonicalize(root).map_err(|error| format!("无法验证附件保险库：{error}"))?;
        Ok(Self {
            inner: Arc::new(AttachmentManagerInner {
                root,
                state: Mutex::new(AttachmentState::default()),
                import_lock: AsyncMutex::new(()),
            }),
        })
    }

    fn activate(&self, scope: AttachmentScope) -> Result<(), String> {
        self.state()?.active_scope = Some(scope);
        Ok(())
    }

    fn deactivate(&self, scope: AttachmentScope) -> Result<(), String> {
        let mut state = self.state()?;
        if state.active_scope.as_ref() == Some(&scope) {
            state.active_scope = None;
        }
        Ok(())
    }

    pub fn active_scope(&self) -> Option<AttachmentScope> {
        self.state().ok()?.active_scope.clone()
    }

    pub fn discard_import_result(&self, result: &AttachmentImportResult) {
        let _ = self.discard(AttachmentIDsInput {
            scope: result.scope.clone(),
            attachment_ids: result
                .attachments
                .iter()
                .map(|attachment| attachment.id.clone())
                .collect(),
        });
    }

    pub async fn stage_paths(
        &self,
        scope: AttachmentScope,
        paths: Vec<PathBuf>,
    ) -> AttachmentImportResult {
        let original_scope = scope.clone();
        let scope = match scope.validated() {
            Ok(scope) => scope,
            Err(error) => {
                return AttachmentImportResult {
                    scope: original_scope,
                    attachments: vec![],
                    errors: vec![error],
                }
            }
        };
        let _guard = self.inner.import_lock.lock().await;
        let mut attachments = Vec::new();
        let mut errors = Vec::new();
        if paths.len() > MAXIMUM_ATTACHMENT_COUNT {
            errors.push(format!("单次最多添加 {MAXIMUM_ATTACHMENT_COUNT} 个附件"));
        }
        for path in paths.into_iter().take(MAXIMUM_ATTACHMENT_COUNT) {
            match read_file_candidate(&path) {
                Ok(candidate) => match self.stage_candidate(&scope, candidate) {
                    Ok(attachment) => attachments.push(attachment),
                    Err(error) => errors.push(error),
                },
                Err(error) => errors.push(error),
            }
        }
        AttachmentImportResult {
            scope,
            attachments,
            errors,
        }
    }

    async fn stage_candidate_for_scope(
        &self,
        scope: AttachmentScope,
        candidate: AttachmentCandidate,
    ) -> AttachmentImportResult {
        let original_scope = scope.clone();
        let scope = match scope.validated() {
            Ok(scope) => scope,
            Err(error) => {
                return AttachmentImportResult {
                    scope: original_scope,
                    attachments: vec![],
                    errors: vec![error],
                }
            }
        };
        let _guard = self.inner.import_lock.lock().await;
        match self.stage_candidate(&scope, candidate) {
            Ok(attachment) => AttachmentImportResult {
                scope,
                attachments: vec![attachment],
                errors: vec![],
            },
            Err(error) => AttachmentImportResult {
                scope,
                attachments: vec![],
                errors: vec![error],
            },
        }
    }

    fn stage_candidate(
        &self,
        scope: &AttachmentScope,
        candidate: AttachmentCandidate,
    ) -> Result<ConversationAttachmentDraft, String> {
        if candidate.bytes.is_empty() {
            return Err(format!("“{}”是空文件", candidate.name));
        }
        if candidate.bytes.len() > MAXIMUM_ATTACHMENT_BYTES {
            return Err(format!("“{}”超过 20 MB", candidate.name));
        }
        let scope_key = scope.key();
        let (count, total_bytes) = {
            let state = self.state()?;
            state
                .drafts
                .iter()
                .filter(|((candidate_scope, _), _)| candidate_scope == &scope_key)
                .fold((0usize, 0usize), |(count, bytes), (_, metadata)| {
                    (count + 1, bytes.saturating_add(metadata.byte_size as usize))
                })
        };
        if count >= MAXIMUM_ATTACHMENT_COUNT {
            return Err(format!("单次最多添加 {MAXIMUM_ATTACHMENT_COUNT} 个附件"));
        }
        if total_bytes.saturating_add(candidate.bytes.len()) > MAXIMUM_ATTACHMENT_BYTES {
            return Err("附件总大小不能超过 20 MB".into());
        }
        let token = Uuid::new_v4().to_string().to_lowercase();
        let attachment_id = format!("attachment_{token}");
        let directory = self.ensure_scope_directory(&scope_key)?;
        let file = directory.join(&token);
        let temporary_file = directory.join(format!("{token}.tmp"));
        if let Err(error) = write_private_file(&temporary_file, &candidate.bytes) {
            let _ = fs::remove_file(&temporary_file);
            return Err(error);
        }
        fs::rename(&temporary_file, &file).map_err(|error| {
            let _ = fs::remove_file(&temporary_file);
            format!("无法保存附件：{error}")
        })?;
        let metadata = VaultMetadata {
            attachment_id: attachment_id.clone(),
            token: token.clone(),
            display_name: sanitized_name(candidate.name),
            media_type: candidate.media_type,
            byte_size: candidate.bytes.len() as u64,
            sha256: digest(&candidate.bytes),
            kind: candidate.kind,
            origin: candidate.origin,
        };
        if let Err(error) = write_metadata(&directory, &metadata) {
            let _ = fs::remove_file(&file);
            return Err(error);
        }
        self.state()?
            .drafts
            .insert((scope_key.clone(), attachment_id), metadata.clone());
        Ok(metadata.draft(&scope_key))
    }

    pub fn resolve_specs(
        &self,
        scope: AttachmentScope,
        attachment_ids: &[String],
    ) -> Result<Vec<LocalConversationAttachmentSpec>, String> {
        let scope = scope.validated()?;
        if attachment_ids.len() > MAXIMUM_ATTACHMENT_COUNT {
            return Err(format!("单次最多添加 {MAXIMUM_ATTACHMENT_COUNT} 个附件"));
        }
        let scope_key = scope.key();
        let state = self.state()?;
        let mut total_bytes = 0usize;
        let mut specs = Vec::with_capacity(attachment_ids.len());
        for attachment_id in attachment_ids {
            let metadata = state
                .drafts
                .get(&(scope_key.clone(), attachment_id.clone()))
                .ok_or_else(|| "附件已失效，请重新添加。".to_string())?;
            self.verify_file(&scope_key, metadata)?;
            total_bytes = total_bytes.saturating_add(metadata.byte_size as usize);
            if total_bytes > MAXIMUM_ATTACHMENT_BYTES {
                return Err("附件总大小不能超过 20 MB".into());
            }
            specs.push(metadata.spec());
        }
        Ok(specs)
    }

    pub fn verified_record_bytes(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        record: &LocalConversationAttachmentRecord,
    ) -> Result<Vec<u8>, String> {
        let scope = AttachmentScope::new(owner_user_id, conversation_id).validated()?;
        let scope_key = scope.key();
        let metadata = self.record_metadata(&scope_key, record)?;
        self.verify_file(&scope_key, &metadata)?;
        self.read_verified_file(&scope_key, &metadata)
    }

    pub fn record_draft(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        record: &LocalConversationAttachmentRecord,
    ) -> Result<ConversationAttachmentDraft, String> {
        let scope = AttachmentScope::new(owner_user_id, conversation_id).validated()?;
        let scope_key = scope.key();
        let metadata = self.record_metadata(&scope_key, record)?;
        self.verify_file(&scope_key, &metadata)?;
        Ok(metadata.draft(&scope_key))
    }

    pub fn record_text_preview(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        record: &LocalConversationAttachmentRecord,
    ) -> Result<AttachmentTextPreview, String> {
        if !is_text_like(&record.media_type) {
            return Err("该附件不支持文本预览。".into());
        }
        let mut bytes = self.verified_record_bytes(owner_user_id, conversation_id, record)?;
        let truncated = bytes.len() > MAXIMUM_TEXT_PREVIEW_BYTES;
        bytes.truncate(MAXIMUM_TEXT_PREVIEW_BYTES);
        Ok(AttachmentTextPreview {
            content: String::from_utf8_lossy(&bytes).into_owned(),
            truncated,
        })
    }

    pub fn commit(&self, scope: AttachmentScope, attachment_ids: &[String]) -> Result<(), String> {
        let scope = scope.validated()?;
        let scope_key = scope.key();
        let mut state = self.state()?;
        for attachment_id in attachment_ids {
            state
                .drafts
                .remove(&(scope_key.clone(), attachment_id.clone()));
        }
        Ok(())
    }

    fn discard(&self, input: AttachmentIDsInput) -> Result<(), String> {
        let scope = input.scope.validated()?;
        let scope_key = scope.key();
        let mut state = self.state()?;
        for attachment_id in input.attachment_ids {
            if let Some(metadata) = state.drafts.remove(&(scope_key.clone(), attachment_id)) {
                let directory = self.validated_scope_directory(&scope_key)?;
                let _ = fs::remove_file(directory.join(&metadata.token));
                let _ = fs::remove_file(metadata_path(&directory, &metadata.token));
            }
        }
        Ok(())
    }

    fn text_preview(&self, input: AttachmentPreviewInput) -> Result<AttachmentTextPreview, String> {
        let scope = input.scope.validated()?;
        let scope_key = scope.key();
        let metadata = self
            .state()?
            .drafts
            .get(&(scope_key.clone(), input.attachment_id))
            .cloned()
            .ok_or_else(|| "附件已失效，请重新添加。".to_string())?;
        if !is_text_like(&metadata.media_type) {
            return Err("该附件不支持文本预览。".into());
        }
        let mut bytes = self.read_verified_file(&scope_key, &metadata)?;
        let truncated = bytes.len() > MAXIMUM_TEXT_PREVIEW_BYTES;
        bytes.truncate(MAXIMUM_TEXT_PREVIEW_BYTES);
        Ok(AttachmentTextPreview {
            content: String::from_utf8_lossy(&bytes).into_owned(),
            truncated,
        })
    }

    fn protocol_response(&self, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
        match self.protocol_body(request.uri().path()) {
            Ok((media_type, body)) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, media_type)
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(body)
                .expect("valid attachment preview response"),
            Err(status) => http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(b"Attachment preview unavailable".to_vec())
                .expect("valid attachment error response"),
        }
    }

    fn protocol_body(&self, path: &str) -> Result<(String, Vec<u8>), http::StatusCode> {
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() != 3
            || !valid_hash(parts[0])
            || !valid_hash(parts[1])
            || !valid_token(parts[2])
        {
            return Err(http::StatusCode::BAD_REQUEST);
        }
        let scope = ScopeKey {
            owner_hash: parts[0].to_owned(),
            conversation_hash: parts[1].to_owned(),
        };
        let directory = self
            .validated_scope_directory(&scope)
            .map_err(|_| http::StatusCode::NOT_FOUND)?;
        let metadata =
            read_metadata(&directory, parts[2]).map_err(|_| http::StatusCode::NOT_FOUND)?;
        if metadata.token != parts[2] || !safe_preview_media_type(&metadata.media_type) {
            return Err(http::StatusCode::FORBIDDEN);
        }
        let body = self
            .read_verified_file(&scope, &metadata)
            .map_err(|_| http::StatusCode::NOT_FOUND)?;
        Ok((metadata.media_type, body))
    }

    fn verify_file(&self, scope: &ScopeKey, metadata: &VaultMetadata) -> Result<(), String> {
        self.read_verified_file(scope, metadata).map(|_| ())
    }

    fn read_verified_file(
        &self,
        scope: &ScopeKey,
        metadata: &VaultMetadata,
    ) -> Result<Vec<u8>, String> {
        let directory = self.validated_scope_directory(scope)?;
        let file = directory.join(&metadata.token);
        let link_metadata =
            fs::symlink_metadata(&file).map_err(|_| "附件已失效，请重新添加。".to_string())?;
        if !link_metadata.file_type().is_file() || link_metadata.file_type().is_symlink() {
            return Err("附件完整性校验失败，请重新添加。".into());
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
        let mut handle = options
            .open(file)
            .map_err(|_| "附件已失效，请重新添加。".to_string())?;
        let before = handle
            .metadata()
            .map_err(|_| "附件完整性校验失败，请重新添加。".to_string())?;
        if !before.file_type().is_file()
            || before.len() != metadata.byte_size
            || metadata.byte_size == 0
            || metadata.byte_size > MAXIMUM_ATTACHMENT_BYTES as u64
        {
            return Err("附件完整性校验失败，请重新添加。".into());
        }
        let mut bytes = Vec::with_capacity(metadata.byte_size as usize);
        (&mut handle)
            .take((MAXIMUM_ATTACHMENT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "附件完整性校验失败，请重新添加。".to_string())?;
        let after = handle
            .metadata()
            .map_err(|_| "附件完整性校验失败，请重新添加。".to_string())?;
        if before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || bytes.len() != metadata.byte_size as usize
            || digest(&bytes) != metadata.sha256
        {
            return Err("附件完整性校验失败，请重新添加。".into());
        }
        Ok(bytes)
    }

    fn record_metadata(
        &self,
        scope: &ScopeKey,
        record: &LocalConversationAttachmentRecord,
    ) -> Result<VaultMetadata, String> {
        let token = record
            .authorized_local_ref
            .strip_prefix(REFERENCE_PREFIX)
            .filter(|token| valid_token(token))
            .ok_or_else(|| "The attachment reference is invalid.".to_string())?;
        let directory = self.validated_scope_directory(scope)?;
        let metadata = read_metadata(&directory, token)
            .map_err(|_| "The authorized attachment is unavailable.".to_string())?;
        if metadata.token != token
            || metadata.attachment_id != record.attachment_id
            || metadata.display_name != record.display_name
            || metadata.media_type != record.media_type
            || metadata.byte_size != record.byte_size
            || metadata.sha256 != record.sha256
        {
            return Err("The authorized attachment failed its integrity check.".into());
        }
        Ok(metadata)
    }

    #[cfg(test)]
    fn scope_directory(&self, scope: &ScopeKey) -> PathBuf {
        self.inner
            .root
            .join(&scope.owner_hash)
            .join(&scope.conversation_hash)
    }

    fn ensure_scope_directory(&self, scope: &ScopeKey) -> Result<PathBuf, String> {
        let owner = self.inner.root.join(&scope.owner_hash);
        ensure_private_directory(&owner)?;
        let directory = owner.join(&scope.conversation_hash);
        ensure_private_directory(&directory)?;
        self.validated_scope_directory(scope)
    }

    fn validated_scope_directory(&self, scope: &ScopeKey) -> Result<PathBuf, String> {
        let owner = self.inner.root.join(&scope.owner_hash);
        let directory = owner.join(&scope.conversation_hash);
        for candidate in [&owner, &directory] {
            let metadata = fs::symlink_metadata(candidate)
                .map_err(|_| "附件保险库作用域不可用。".to_string())?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err("附件保险库作用域无效。".into());
            }
        }
        let canonical =
            fs::canonicalize(&directory).map_err(|_| "附件保险库作用域不可用。".to_string())?;
        if !canonical.starts_with(&self.inner.root) || canonical != directory {
            return Err("附件保险库作用域无效。".into());
        }
        Ok(directory)
    }

    fn state(&self) -> Result<std::sync::MutexGuard<'_, AttachmentState>, String> {
        self.inner
            .state
            .lock()
            .map_err(|_| "附件状态不可用。".to_string())
    }
}

struct AttachmentCandidate {
    name: String,
    media_type: String,
    kind: AttachmentKind,
    origin: AttachmentOrigin,
    bytes: Vec<u8>,
}

enum ClipboardPayload {
    Image(AttachmentCandidate),
    LongText(AttachmentCandidate),
    Text(String),
    Empty,
}

#[tauri::command]
pub async fn activate_conversation_attachment_scope(
    manager: State<'_, AttachmentManager>,
    input: AttachmentScope,
) -> Result<(), String> {
    manager.activate(input.validated()?)
}

#[tauri::command]
pub async fn deactivate_conversation_attachment_scope(
    manager: State<'_, AttachmentManager>,
    input: AttachmentScope,
) -> Result<(), String> {
    manager.deactivate(input.validated()?)
}

#[tauri::command]
pub async fn pick_conversation_attachments(
    manager: State<'_, AttachmentManager>,
    input: AttachmentScope,
) -> Result<AttachmentImportResult, String> {
    let scope = input.validated()?;
    let paths = rfd::AsyncFileDialog::new()
        .set_title("选择附件")
        .pick_files()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|handle| handle.path().to_path_buf())
        .collect();
    Ok(manager.stage_paths(scope, paths).await)
}

#[tauri::command]
pub async fn import_conversation_clipboard(
    manager: State<'_, AttachmentManager>,
    input: AttachmentScope,
) -> Result<ClipboardImportResult, String> {
    let scope = input.validated()?;
    let payload = tokio::task::spawn_blocking(read_clipboard_payload)
        .await
        .map_err(|error| format!("无法读取剪贴板：{error}"))??;
    match payload {
        ClipboardPayload::Image(candidate) | ClipboardPayload::LongText(candidate) => {
            let result = manager.stage_candidate_for_scope(scope, candidate).await;
            Ok(ClipboardImportResult {
                kind: "attachments",
                text: None,
                result: Some(result),
            })
        }
        ClipboardPayload::Text(text) => Ok(ClipboardImportResult {
            kind: "text",
            text: Some(text),
            result: None,
        }),
        ClipboardPayload::Empty => Ok(ClipboardImportResult {
            kind: "empty",
            text: None,
            result: None,
        }),
    }
}

#[tauri::command]
pub async fn discard_conversation_attachments(
    manager: State<'_, AttachmentManager>,
    input: AttachmentIDsInput,
) -> Result<(), String> {
    manager.discard(input)
}

#[tauri::command]
pub async fn preview_conversation_attachment_text(
    manager: State<'_, AttachmentManager>,
    input: AttachmentPreviewInput,
) -> Result<AttachmentTextPreview, String> {
    manager.text_preview(input)
}

#[tauri::command]
pub async fn load_persisted_conversation_attachments(
    host: State<'_, HostManager>,
    manager: State<'_, AttachmentManager>,
    input: PersistedAttachmentsInput,
) -> Result<Vec<ConversationAttachmentDraft>, String> {
    let scope = input.scope.validated()?;
    if input.authorized_local_refs.len() > MAXIMUM_ATTACHMENT_COUNT {
        return Err(format!("单次最多读取 {MAXIMUM_ATTACHMENT_COUNT} 个附件"));
    }
    let mut attachments = Vec::with_capacity(input.authorized_local_refs.len());
    for authorized_local_ref in input.authorized_local_refs {
        let record = exact_attachment_record(
            &host,
            &scope,
            required("authorizedLocalRef", authorized_local_ref)?,
        )
        .await?;
        attachments.push(manager.record_draft(
            &scope.owner_user_id,
            &scope.conversation_id,
            &record,
        )?);
    }
    Ok(attachments)
}

#[tauri::command]
pub async fn preview_persisted_conversation_attachment_text(
    host: State<'_, HostManager>,
    manager: State<'_, AttachmentManager>,
    input: PersistedAttachmentPreviewInput,
) -> Result<AttachmentTextPreview, String> {
    let scope = input.scope.validated()?;
    let record = exact_attachment_record(
        &host,
        &scope,
        required("authorizedLocalRef", input.authorized_local_ref)?,
    )
    .await?;
    manager.record_text_preview(&scope.owner_user_id, &scope.conversation_id, &record)
}

pub fn attachment_protocol(
    manager: AttachmentManager,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    manager.protocol_response(request)
}

async fn exact_attachment_record(
    host: &HostManager,
    scope: &AttachmentScope,
    authorized_local_ref: String,
) -> Result<LocalConversationAttachmentRecord, String> {
    let result = host
        .execute(HostCommand::GetConversationAttachment(
            GetConversationAttachmentCommand {
                owner_user_id: scope.owner_user_id.clone(),
                conversation_id: scope.conversation_id.clone(),
                authorized_local_ref,
            },
        ))
        .await?;
    let HostResult::ConversationAttachment { attachment } = result else {
        return Err("Local Agent Host 返回了错误的附件响应。".into());
    };
    attachment.ok_or_else(|| "附件不存在或不属于当前会话。".into())
}

fn read_file_candidate(path: &Path) -> Result<AttachmentCandidate, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitized_name)
        .unwrap_or_else(|| "附件".into());
    let link_metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法读取“{name}”：{error}"))?;
    if !link_metadata.file_type().is_file() || link_metadata.file_type().is_symlink() {
        return Err(format!("“{name}”不是可发送的文件"));
    }
    if link_metadata.len() > MAXIMUM_ATTACHMENT_BYTES as u64 {
        return Err(format!("“{name}”超过 20 MB"));
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
    let mut handle = options
        .open(path)
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    let before = handle
        .metadata()
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    if !before.file_type().is_file()
        || before.len() == 0
        || before.len() != link_metadata.len()
        || before.len() > MAXIMUM_ATTACHMENT_BYTES as u64
    {
        return Err(format!("“{name}”不是可发送的文件"));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut handle)
        .take((MAXIMUM_ATTACHMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    let after = handle
        .metadata()
        .map_err(|error| format!("无法读取“{name}”：{error}"))?;
    if bytes.len() > MAXIMUM_ATTACHMENT_BYTES {
        return Err(format!("“{name}”超过 20 MB"));
    }
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
    {
        return Err(format!("“{name}”在读取期间发生了变化，请重新添加"));
    }
    let media_type = mime_guess::from_path(&name)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_owned();
    Ok(AttachmentCandidate {
        name,
        kind: attachment_kind(&media_type),
        media_type,
        origin: AttachmentOrigin::File,
        bytes,
    })
}

fn read_clipboard_payload() -> Result<ClipboardPayload, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| format!("无法访问剪贴板：{error}"))?;
    if let Ok(image) = clipboard.get_image() {
        let expected = image
            .width
            .checked_mul(image.height)
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| "剪贴板图片尺寸无效。".to_string())?;
        if expected != image.bytes.len() || expected > 160 * 1_024 * 1_024 {
            return Err("剪贴板图片过大或格式无效。".into());
        }
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(
                Cursor::new(&mut encoded),
                image.width as u32,
                image.height as u32,
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|error| format!("无法处理剪贴板图片：{error}"))?;
            writer
                .write_image_data(image.bytes.as_ref())
                .map_err(|error| format!("无法处理剪贴板图片：{error}"))?;
        }
        return Ok(ClipboardPayload::Image(AttachmentCandidate {
            name: pasted_name("粘贴的图片", "png"),
            media_type: "image/png".into(),
            kind: AttachmentKind::Image,
            origin: AttachmentOrigin::PastedImage,
            bytes: encoded,
        }));
    }
    match clipboard.get_text() {
        Ok(text) if !text.is_empty() => {
            if text.chars().count() >= LONG_PASTE_CHARACTER_THRESHOLD
                || text.len() >= LONG_PASTE_BYTE_THRESHOLD
            {
                Ok(ClipboardPayload::LongText(AttachmentCandidate {
                    name: pasted_name("粘贴的长文本", "txt"),
                    media_type: "text/plain".into(),
                    kind: AttachmentKind::File,
                    origin: AttachmentOrigin::PastedText,
                    bytes: text.into_bytes(),
                }))
            } else {
                Ok(ClipboardPayload::Text(text))
            }
        }
        _ => Ok(ClipboardPayload::Empty),
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir(path).map_err(|error| format!("无法创建附件保险库：{error}"))?;
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法验证附件保险库：{error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("附件保险库目录无效。".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("无法保护附件保险库：{error}"))?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("无法保存附件：{error}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("无法保存附件：{error}"))
}

fn write_metadata(directory: &Path, metadata: &VaultMetadata) -> Result<(), String> {
    let bytes =
        serde_json::to_vec(metadata).map_err(|error| format!("无法记录附件元数据：{error}"))?;
    let target = metadata_path(directory, &metadata.token);
    let temporary = directory.join(format!("{}.json.tmp", metadata.token));
    write_private_file(&temporary, &bytes)?;
    fs::rename(&temporary, target).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("无法记录附件元数据：{error}")
    })
}

fn read_metadata(directory: &Path, token: &str) -> Result<VaultMetadata, String> {
    let bytes = fs::read(metadata_path(directory, token))
        .map_err(|error| format!("无法读取附件元数据：{error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("附件元数据无效：{error}"))
}

fn metadata_path(directory: &Path, token: &str) -> PathBuf {
    directory.join(format!("{token}.json"))
}

fn attachment_kind(media_type: &str) -> AttachmentKind {
    if media_type.starts_with("image/") {
        AttachmentKind::Image
    } else if media_type.starts_with("audio/") {
        AttachmentKind::Audio
    } else {
        AttachmentKind::File
    }
}

fn safe_preview_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

fn is_text_like(media_type: &str) -> bool {
    media_type.starts_with("text/")
        || matches!(
            media_type,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/x-yaml"
        )
}

fn preview_url(scope: &ScopeKey, token: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!(
            "http://chatos-attachment.localhost/{}/{}/{}",
            scope.owner_hash, scope.conversation_hash, token
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!(
            "chatos-attachment://localhost/{}/{}/{}",
            scope.owner_hash, scope.conversation_hash, token
        )
    }
}

fn sanitized_name(value: impl AsRef<str>) -> String {
    let value = value
        .as_ref()
        .chars()
        .filter(|character| !character.is_control())
        .take(255)
        .collect::<String>();
    if value.trim().is_empty() {
        "附件".into()
    } else {
        value
    }
}

fn pasted_name(prefix: &str, extension: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("{prefix} {timestamp}.{extension}")
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_token(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.to_string().to_lowercase() == value)
        .unwrap_or(false)
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> (tempfile::TempDir, AttachmentManager) {
        let directory = tempfile::tempdir().unwrap();
        let manager = AttachmentManager::new(directory.path().join("vault")).unwrap();
        (directory, manager)
    }

    fn scope() -> AttachmentScope {
        AttachmentScope {
            owner_user_id: "owner-1".into(),
            conversation_id: "conversation-1".into(),
            draft_generation: 1,
        }
    }

    fn candidate(bytes: Vec<u8>) -> AttachmentCandidate {
        AttachmentCandidate {
            name: "notes.txt".into(),
            media_type: "text/plain".into(),
            kind: AttachmentKind::File,
            origin: AttachmentOrigin::File,
            bytes,
        }
    }

    #[tokio::test]
    async fn stages_opaque_owner_scoped_attachment_and_resolves_exact_spec() {
        let (_directory, manager) = manager();
        let result = manager
            .stage_candidate_for_scope(scope(), candidate(b"hello".to_vec()))
            .await;
        assert!(result.errors.is_empty());
        let draft = &result.attachments[0];
        assert!(!draft.id.contains("notes.txt"));
        assert!(draft.preview_url.is_none());

        let specs = manager
            .resolve_specs(scope(), std::slice::from_ref(&draft.id))
            .unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].display_name, "notes.txt");
        assert_eq!(specs[0].byte_size, 5);
        assert_eq!(specs[0].sha256, digest(b"hello"));
        assert!(specs[0].authorized_local_ref.starts_with(REFERENCE_PREFIX));
    }

    #[tokio::test]
    async fn rejects_cross_owner_resolution_and_tampering() {
        let (_directory, manager) = manager();
        let result = manager
            .stage_candidate_for_scope(scope(), candidate(b"hello".to_vec()))
            .await;
        let draft = &result.attachments[0];
        let other = AttachmentScope {
            owner_user_id: "owner-2".into(),
            conversation_id: "conversation-1".into(),
            draft_generation: 1,
        };
        assert!(manager
            .resolve_specs(other, std::slice::from_ref(&draft.id))
            .is_err());

        let metadata = manager
            .state()
            .unwrap()
            .drafts
            .get(&(scope().key(), draft.id.clone()))
            .cloned()
            .unwrap();
        fs::write(
            manager.scope_directory(&scope().key()).join(metadata.token),
            b"other",
        )
        .unwrap();
        assert!(manager
            .resolve_specs(scope(), std::slice::from_ref(&draft.id))
            .is_err());
    }

    #[tokio::test]
    async fn enforces_count_and_aggregate_byte_limits() {
        let (_directory, manager) = manager();
        let first = manager
            .stage_candidate_for_scope(scope(), candidate(vec![0; MAXIMUM_ATTACHMENT_BYTES - 1]))
            .await;
        assert_eq!(first.attachments.len(), 1);
        let second = manager
            .stage_candidate_for_scope(scope(), candidate(vec![0; 2]))
            .await;
        assert!(second.attachments.is_empty());
        assert_eq!(second.errors, ["附件总大小不能超过 20 MB"]);
    }

    #[tokio::test]
    async fn discard_removes_uncommitted_files_but_commit_preserves_them() {
        let (_directory, manager) = manager();
        let discarded = manager
            .stage_candidate_for_scope(scope(), candidate(b"discard".to_vec()))
            .await
            .attachments
            .remove(0);
        let metadata = manager
            .state()
            .unwrap()
            .drafts
            .get(&(scope().key(), discarded.id.clone()))
            .cloned()
            .unwrap();
        let file = manager
            .scope_directory(&scope().key())
            .join(&metadata.token);
        manager
            .discard(AttachmentIDsInput {
                scope: scope(),
                attachment_ids: vec![discarded.id],
            })
            .unwrap();
        assert!(!file.exists());

        let committed = manager
            .stage_candidate_for_scope(scope(), candidate(b"commit".to_vec()))
            .await
            .attachments
            .remove(0);
        let metadata = manager
            .state()
            .unwrap()
            .drafts
            .get(&(scope().key(), committed.id.clone()))
            .cloned()
            .unwrap();
        let file = manager
            .scope_directory(&scope().key())
            .join(&metadata.token);
        manager.commit(scope(), &[committed.id]).unwrap();
        assert!(file.exists());
    }
}

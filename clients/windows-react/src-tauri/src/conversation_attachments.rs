mod cleanup;
mod commands;
mod platform;
#[cfg(test)]
mod tests;

use cleanup::cleanup_attachment_temporaries;
pub use commands::{
    activate_conversation_attachment_scope, attachment_protocol,
    deactivate_conversation_attachment_scope, discard_conversation_attachments,
    import_conversation_clipboard, load_persisted_conversation_attachments,
    pick_conversation_attachments, preview_conversation_attachment_text,
    preview_persisted_conversation_attachment_text,
};
use platform::*;

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
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{http, State};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::{
    app_data_transaction::{create_owned_file, ensure_owned_directory, replace_owned_file},
    host_lifecycle::HostManager,
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePriority,
    },
};

const MAXIMUM_ATTACHMENT_COUNT: usize = 20;
const MAXIMUM_ATTACHMENT_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_ATTACHMENT_METADATA_BYTES: usize = 64 * 1_024;
const LONG_PASTE_CHARACTER_THRESHOLD: usize = 4_000;
const LONG_PASTE_BYTE_THRESHOLD: usize = 8_000;
const MAXIMUM_TEXT_PREVIEW_BYTES: usize = 16 * 1_024;
const REFERENCE_PREFIX: &str = "local-attachment:";
const STALE_TEMPORARY_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAXIMUM_CLEANUP_ENTRIES: usize = 20_000;
const MAXIMUM_CLEANUP_DEPTH: usize = 2;

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

#[derive(Debug, Clone)]
pub(crate) struct VerifiedAttachmentPayload {
    pub(crate) display_name: String,
    pub(crate) media_type: String,
    pub(crate) kind: String,
    pub(crate) origin: String,
    pub(crate) data: Vec<u8>,
    pub(crate) sha256: String,
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
            ensure_owned_directory(parent)
                .map_err(|error| format!("无法创建应用数据目录：{error}"))?;
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

    pub async fn cleanup_stale_temporaries(&self) -> usize {
        let root = self.inner.root.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let cutoff = SystemTime::now()
                .checked_sub(STALE_TEMPORARY_AGE)
                .unwrap_or(UNIX_EPOCH);
            cleanup_attachment_temporaries(&root, cutoff)
        })
        .await
        .unwrap_or_default()
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
        let manager = self.clone();
        let failed_scope = scope.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut attachments = Vec::new();
            let mut errors = Vec::new();
            if paths.len() > MAXIMUM_ATTACHMENT_COUNT {
                errors.push(format!("单次最多添加 {MAXIMUM_ATTACHMENT_COUNT} 个附件"));
            }
            for path in paths.into_iter().take(MAXIMUM_ATTACHMENT_COUNT) {
                match read_file_candidate(&path) {
                    Ok(candidate) => match manager.stage_candidate(&scope, candidate) {
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
        })
        .await
        .unwrap_or_else(|_| AttachmentImportResult {
            scope: failed_scope,
            attachments: Vec::new(),
            errors: vec!["附件读取任务异常退出。".into()],
        })
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
        let manager = self.clone();
        let failed_scope = scope.clone();
        tauri::async_runtime::spawn_blocking(move || {
            match manager.stage_candidate(&scope, candidate) {
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
        })
        .await
        .unwrap_or_else(|_| AttachmentImportResult {
            scope: failed_scope,
            attachments: Vec::new(),
            errors: vec!["附件保存任务异常退出。".into()],
        })
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
        create_owned_file(&directory, &file, ".attachment-payload-", &candidate.bytes)
            .map_err(|error| format!("无法保存附件：{error}"))?;
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

    pub(crate) fn verified_draft_payloads(
        &self,
        scope: AttachmentScope,
        attachment_ids: &[String],
    ) -> Result<Vec<VerifiedAttachmentPayload>, String> {
        let scope = scope.validated()?;
        if attachment_ids.len() > MAXIMUM_ATTACHMENT_COUNT {
            return Err(format!("单次最多添加 {MAXIMUM_ATTACHMENT_COUNT} 个附件"));
        }
        let scope_key = scope.key();
        let state = self.state()?;
        let mut unique_ids = std::collections::HashSet::with_capacity(attachment_ids.len());
        let mut total_bytes = 0_usize;
        let mut payloads = Vec::with_capacity(attachment_ids.len());
        for attachment_id in attachment_ids {
            if !unique_ids.insert(attachment_id) {
                return Err("团队消息附件不能重复。".into());
            }
            let metadata = state
                .drafts
                .get(&(scope_key.clone(), attachment_id.clone()))
                .ok_or_else(|| "附件已失效，请重新添加。".to_string())?;
            let data = self.read_verified_file(&scope_key, metadata)?;
            total_bytes = total_bytes.saturating_add(data.len());
            if total_bytes > MAXIMUM_ATTACHMENT_BYTES {
                return Err("附件总大小不能超过 20 MB".into());
            }
            payloads.push(VerifiedAttachmentPayload {
                display_name: metadata.display_name.clone(),
                media_type: metadata.media_type.clone(),
                kind: match metadata.kind {
                    AttachmentKind::Image => "image",
                    AttachmentKind::File => "file",
                    AttachmentKind::Audio => "audio",
                }
                .into(),
                origin: match metadata.origin {
                    AttachmentOrigin::File => "file",
                    AttachmentOrigin::PastedImage => "pastedImage",
                    AttachmentOrigin::PastedText => "pastedText",
                }
                .into(),
                sha256: metadata.sha256.clone(),
                data,
            });
        }
        Ok(payloads)
    }

    pub(crate) fn authorize_verified_payloads(
        &self,
        scope: AttachmentScope,
        payloads: &[VerifiedAttachmentPayload],
    ) -> Result<(Vec<String>, Vec<LocalConversationAttachmentSpec>), String> {
        let scope = scope.validated()?;
        if payloads.len() > MAXIMUM_ATTACHMENT_COUNT {
            return Err(format!("单次最多添加 {MAXIMUM_ATTACHMENT_COUNT} 个附件"));
        }
        if payloads.iter().any(|payload| {
            payload.data.is_empty()
                || !matches!(payload.kind.as_str(), "image" | "file" | "audio")
                || !matches!(
                    payload.origin.as_str(),
                    "file" | "pastedImage" | "pastedText"
                )
                || digest(&payload.data) != payload.sha256
        }) {
            return Err("团队消息附件完整性校验失败。".into());
        }
        let mut total_bytes = 0_usize;
        let mut attachment_ids = Vec::with_capacity(payloads.len());
        for payload in payloads {
            total_bytes = total_bytes.saturating_add(payload.data.len());
            if total_bytes > MAXIMUM_ATTACHMENT_BYTES {
                let _ = self.discard(AttachmentIDsInput {
                    scope: scope.clone(),
                    attachment_ids,
                });
                return Err("团队消息附件完整性校验失败。".into());
            }
            let kind = match payload.kind.as_str() {
                "image" => AttachmentKind::Image,
                "file" => AttachmentKind::File,
                "audio" => AttachmentKind::Audio,
                _ => return Err("团队消息附件类型无效。".into()),
            };
            let origin = match payload.origin.as_str() {
                "file" => AttachmentOrigin::File,
                "pastedImage" => AttachmentOrigin::PastedImage,
                "pastedText" => AttachmentOrigin::PastedText,
                _ => return Err("团队消息附件来源无效。".into()),
            };
            match self.stage_candidate(
                &scope,
                AttachmentCandidate {
                    name: payload.display_name.clone(),
                    media_type: payload.media_type.clone(),
                    kind,
                    origin,
                    bytes: payload.data.clone(),
                },
            ) {
                Ok(draft) => attachment_ids.push(draft.id),
                Err(error) => {
                    let _ = self.discard(AttachmentIDsInput {
                        scope: scope.clone(),
                        attachment_ids,
                    });
                    return Err(error);
                }
            }
        }
        match self.resolve_specs(scope.clone(), &attachment_ids) {
            Ok(specs) => Ok((attachment_ids, specs)),
            Err(error) => {
                let _ = self.discard(AttachmentIDsInput {
                    scope,
                    attachment_ids,
                });
                Err(error)
            }
        }
    }

    pub(crate) fn discard_ids(&self, scope: AttachmentScope, attachment_ids: Vec<String>) {
        let _ = self.discard(AttachmentIDsInput {
            scope,
            attachment_ids,
        });
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
}

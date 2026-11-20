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
    resources: State<'_, MachineResourceGovernor>,
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
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    Ok(manager.stage_paths(scope, paths).await)
}

#[tauri::command]
pub async fn import_conversation_clipboard(
    manager: State<'_, AttachmentManager>,
    resources: State<'_, MachineResourceGovernor>,
    input: AttachmentScope,
) -> Result<ClipboardImportResult, String> {
    let scope = input.validated()?;
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
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

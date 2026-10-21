mod history;
mod provider;
mod reference;
mod video_history;
mod video_input;
mod video_job;
mod video_provider;

use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc, Mutex},
};
use tauri::{http, State};
use uuid::Uuid;

use crate::{
    authentication::{AuthenticationManager, GatewayModelConfig},
    host_lifecycle::validate_owner,
};
use history::{
    ImageGenerationOutcome, ImageHistoryCursor, ImageHistoryItem, ImageHistoryPage,
    MediaStudioHistoryStore,
};
use provider::{
    make_provider_request, parse_provider_response, provider_error_detail, resolve_image_model,
    ImageGenerationRequest,
};
use reference::{MediaImageReferenceImportResult, MediaImageReferenceVault};
use video_history::{
    MediaStudioVideoHistoryStore, VideoHistoryCursor, VideoHistoryItem, VideoHistoryPage,
};
use video_input::{MediaVideoAudioDraft, MediaVideoInputVault};
use video_job::{
    MediaStudioVideoJobRegistry, MediaStudioVideoJobSnapshot, VideoPollStart, VideoPollWork,
};
use video_provider::{
    make_video_create_request, make_video_result, parse_video_job, resolve_video_model,
    video_content_endpoint, video_job_endpoint, VideoGenerationRequest, VideoInput,
    MAXIMUM_VIDEO_BYTES, MAXIMUM_VIDEO_JOB_RESPONSE_BYTES,
};

const MAXIMUM_PROVIDER_RESPONSE_BYTES: usize = 48 * 1_024 * 1_024;
const MAXIMUM_IMAGE_REFERENCES: usize = 8;

#[derive(Clone)]
pub(crate) struct MediaStudioManager {
    history: MediaStudioHistoryStore,
    video_history: MediaStudioVideoHistoryStore,
    provider_client: reqwest::Client,
    preview_session: Arc<Mutex<Option<MediaPreviewSession>>>,
    references: MediaImageReferenceVault,
    video_inputs: MediaVideoInputVault,
    video_jobs: MediaStudioVideoJobRegistry,
}

pub(crate) struct StoryGeneratedImage {
    pub bytes: Vec<u8>,
    pub provider_result_id: String,
    pub provider_asset_id: String,
}

pub(crate) struct StoryImageReference {
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
struct MediaPreviewSession {
    owner_hash: String,
    authentication_generation: u64,
    token: String,
}

impl MediaStudioManager {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let provider_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(10 * 60))
            .build()
            .map_err(|_| "无法初始化图片生成网络客户端。".to_owned())?;
        Ok(Self {
            history: MediaStudioHistoryStore::new(app_data_directory.clone())?,
            video_history: MediaStudioVideoHistoryStore::new(app_data_directory.clone()),
            provider_client,
            preview_session: Arc::new(Mutex::new(None)),
            references: MediaImageReferenceVault::new(
                app_data_directory
                    .join("MediaStudio")
                    .join("ReferenceVault"),
            )?,
            video_inputs: MediaVideoInputVault::new(
                app_data_directory
                    .join("MediaStudio")
                    .join("VideoInputVault"),
            )?,
            video_jobs: MediaStudioVideoJobRegistry::default(),
        })
    }

    pub(crate) fn from_runtime() -> Result<Self, String> {
        let app_data_directory = dirs::data_local_dir()
            .ok_or_else(|| "无法定位应用数据目录。".to_owned())?
            .join("com.chatos.windows");
        Self::new(app_data_directory)
    }

    fn activate_session(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
    ) -> Result<String, String> {
        let owner_hash = history::owner_hash(owner_user_id);
        let mut session = self
            .preview_session
            .lock()
            .map_err(|_| "图片预览状态不可用。".to_owned())?;
        if let Some(active) = session.as_ref() {
            if active.owner_hash == owner_hash
                && active.authentication_generation == authentication_generation
            {
                return Ok(active.token.clone());
            }
        }
        self.references.reset();
        self.video_inputs.reset();
        self.video_jobs.reset();
        let token = Uuid::new_v4().to_string();
        *session = Some(MediaPreviewSession {
            owner_hash,
            authentication_generation,
            token: token.clone(),
        });
        Ok(token)
    }

    pub(crate) fn reset(&self) {
        if let Ok(mut session) = self.preview_session.lock() {
            *session = None;
        }
        self.references.reset();
        self.video_inputs.reset();
        self.video_jobs.reset();
    }

    /// Reads one already-persisted generated image through Media Studio's owner-scoped history
    /// boundary. Story Studio deliberately receives bytes rather than a path or provider asset so
    /// the history layout and provider identities never become part of its IPC contract.
    pub(crate) fn read_story_history_image(
        &self,
        owner_user_id: &str,
        record_id: &str,
        content_token: &str,
    ) -> Result<Vec<u8>, String> {
        validate_owner(owner_user_id)?;
        self.history
            .read_history_asset(
                &history::owner_hash(owner_user_id),
                record_id,
                content_token,
            )
            .map(|(_, bytes)| bytes)
            .map_err(|_| "所选生成图片不存在或不属于当前账号。".to_owned())
    }

    /// Narrow provider port used by Story Studio. Model credentials, wire payloads, provider
    /// URLs and encoded media remain inside the native Media Studio adapter.
    pub(crate) async fn generate_story_image(
        &self,
        authentication: &AuthenticationManager,
        owner_user_id: &str,
        authentication_generation: u64,
        model_config_id: &str,
        prompt: String,
        client_request_id: &str,
        references: Vec<StoryImageReference>,
    ) -> Result<StoryGeneratedImage, String> {
        authentication
            .require_session_generation(owner_user_id, authentication_generation)
            .await?;
        let mut models = authentication
            .resolved_models_for_owner(owner_user_id, vec![model_config_id.to_owned()])
            .await
            .map_err(|error| error.message)?;
        if models.len() != 1 {
            return Err("剧情图片模型已不可用。".into());
        }
        let runtime = resolve_image_model(owner_user_id, model_config_id, models.remove(0))?;
        authentication.save_model_credential(
            owner_user_id,
            &runtime.model_config_id,
            &runtime.api_key,
        )?;
        authentication
            .require_session_generation(owner_user_id, authentication_generation)
            .await?;
        let request = ImageGenerationRequest {
            model_config_id: model_config_id.to_owned(),
            prompt,
            size: None,
            count: 1,
            reference_images: references
                .into_iter()
                .map(|reference| provider::ImageGenerationInput {
                    name: reference.name,
                    mime_type: reference.mime_type,
                    bytes: reference.bytes,
                })
                .collect(),
            client_request_id: client_request_id.to_owned(),
        };
        let wire = make_provider_request(&runtime, &request)?;
        let response = self
            .provider_client
            .post(wire.url)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, wire.content_type)
            .bearer_auth(&runtime.api_key)
            .body(wire.body)
            .send()
            .await
            .map_err(|_| "无法连接剧情图片模型服务。".to_owned())?;
        let status = response.status();
        let body = read_provider_response(response).await?;
        if !status.is_success() {
            return Err(format!(
                "剧情图片生成失败（HTTP {}）：{}",
                status.as_u16(),
                provider_error_detail(&body, &runtime.api_key)
            ));
        }
        let result = parse_provider_response(&request, &runtime, &body)?;
        authentication
            .require_session_generation(owner_user_id, authentication_generation)
            .await?;
        if result.client_request_id != client_request_id {
            return Err("剧情图片结果与当前生成请求不匹配。".into());
        }
        let asset = result
            .images
            .first()
            .ok_or_else(|| "剧情图片模型没有返回图片。".to_owned())?;
        let bytes = self.history.asset_bytes(asset).await?;
        authentication
            .require_session_generation(owner_user_id, authentication_generation)
            .await?;
        Ok(StoryGeneratedImage {
            bytes,
            provider_result_id: result.id,
            provider_asset_id: asset.id.clone(),
        })
    }

    fn present_history_item(
        &self,
        mut item: ImageHistoryItem,
        preview_session_token: &str,
    ) -> ImageHistoryItem {
        for asset in &mut item.images {
            asset.preview_url = Some(media_preview_url(
                preview_session_token,
                &item.id,
                &asset.content_token,
            ));
        }
        item
    }

    fn present_video_history_item(
        &self,
        mut item: VideoHistoryItem,
        preview_session_token: &str,
    ) -> VideoHistoryItem {
        item.preview_url = Some(media_preview_url(
            preview_session_token,
            &item.id,
            &item.content_token,
        ));
        item
    }

    fn protocol_response(&self, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
        match self.protocol_body(request.uri().path()) {
            Ok((mime_type, body)) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, mime_type)
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(body)
                .expect("valid media preview response"),
            Err(status) => http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"Media preview unavailable".to_vec())
                .expect("valid media preview error response"),
        }
    }

    fn protocol_body(&self, path: &str) -> Result<(String, Vec<u8>), http::StatusCode> {
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() != 3 && parts.len() != 4 {
            return Err(http::StatusCode::BAD_REQUEST);
        }
        let (owner_hash, authentication_generation) = {
            let session = self
                .preview_session
                .lock()
                .map_err(|_| http::StatusCode::SERVICE_UNAVAILABLE)?;
            let active = session.as_ref().ok_or(http::StatusCode::UNAUTHORIZED)?;
            if !valid_uuid(parts[0]) || active.token != parts[0] {
                return Err(http::StatusCode::UNAUTHORIZED);
            }
            (active.owner_hash.clone(), active.authentication_generation)
        };
        match parts.as_slice() {
            [_, record_id, content_token]
                if valid_uuid(record_id) && valid_content_token(content_token) =>
            {
                self.history
                    .read_history_asset(&owner_hash, record_id, content_token)
                    .or_else(|_| {
                        self.video_history
                            .read_asset(&owner_hash, record_id, content_token)
                    })
                    .map_err(|_| http::StatusCode::NOT_FOUND)
            }
            [_, "reference", presentation_id, token]
                if valid_uuid(presentation_id) && valid_uuid(token) =>
            {
                self.references
                    .preview_asset(
                        &owner_hash,
                        authentication_generation,
                        presentation_id,
                        token,
                    )
                    .map_err(|_| http::StatusCode::NOT_FOUND)
            }
            _ => Err(http::StatusCode::BAD_REQUEST),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioImageGenerationRequest {
    presentation_id: String,
    model_config_id: String,
    prompt: String,
    size: Option<String>,
    count: u8,
    #[serde(default)]
    reference_image_ids: Vec<String>,
    client_request_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioVideoGenerationRequest {
    presentation_id: String,
    model_config_id: String,
    prompt: String,
    size: String,
    seconds: u8,
    ratio: String,
    input_image_id: Option<String>,
    reference_audio_id: Option<String>,
    client_request_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioReferenceSelectionInput {
    owner_user_id: String,
    presentation_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioReferenceDiscardInput {
    owner_user_id: String,
    presentation_id: String,
    reference_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioVideoAudioDiscardInput {
    owner_user_id: String,
    presentation_id: String,
    reference_audio_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioVideoJobInput {
    owner_user_id: String,
    presentation_id: String,
    job_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioHistoryReferenceInput {
    record_id: String,
    content_token: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaStudioHistoryReferenceSelectionInput {
    owner_user_id: String,
    presentation_id: String,
    assets: Vec<MediaStudioHistoryReferenceInput>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaStudioModelOption {
    id: String,
    name: String,
    provider: String,
    model_name: String,
    enabled: bool,
    task_enabled: bool,
    has_api_key: bool,
}

#[tauri::command]
pub(crate) async fn list_media_studio_models(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    owner_user_id: String,
) -> Result<Vec<MediaStudioModelOption>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    manager.activate_session(&owner_user_id, generation)?;
    let catalog = authentication
        .model_catalog_for_owner(&owner_user_id)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    present_model_catalog(&owner_user_id, catalog)
}

#[tauri::command]
pub(crate) async fn load_media_studio_image_history(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    owner_user_id: String,
    after: Option<ImageHistoryCursor>,
    limit: Option<usize>,
) -> Result<ImageHistoryPage, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let preview_session_token = manager.activate_session(&owner_user_id, generation)?;
    let history = manager.history.clone();
    let load_owner = owner_user_id.clone();
    let mut page = tauri::async_runtime::spawn_blocking(move || {
        history.load_image_page(&load_owner, after, limit.unwrap_or(10))
    })
    .await
    .map_err(|_| "图片历史读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    for item in &mut page.items {
        *item = manager.present_history_item(item.clone(), &preview_session_token);
    }
    Ok(page)
}

#[tauri::command]
pub(crate) async fn load_media_studio_video_history(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    owner_user_id: String,
    after: Option<VideoHistoryCursor>,
    limit: Option<usize>,
) -> Result<VideoHistoryPage, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let preview_session_token = manager.activate_session(&owner_user_id, generation)?;
    let history = manager.video_history.clone();
    let load_owner = owner_user_id.clone();
    let mut page = tauri::async_runtime::spawn_blocking(move || {
        history.load_page(&load_owner, after, limit.unwrap_or(10))
    })
    .await
    .map_err(|_| "视频历史读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    for item in &mut page.items {
        *item = manager.present_video_history_item(item.clone(), &preview_session_token);
    }
    Ok(page)
}

#[tauri::command]
pub(crate) async fn pick_media_studio_reference_images(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioReferenceSelectionInput,
) -> Result<MediaImageReferenceImportResult, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let preview_session_token = manager.activate_session(&input.owner_user_id, generation)?;
    let (epoch, presentation_id) =
        manager
            .references
            .begin_scope(&input.owner_user_id, generation, &input.presentation_id)?;
    let paths = rfd::AsyncFileDialog::new()
        .set_title("选择参考图")
        .add_filter("图片", &["png", "jpg", "jpeg", "webp"])
        .pick_files()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|handle| handle.path().to_path_buf())
        .collect::<Vec<_>>();
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let references = manager.references.clone();
    let owner_user_id = input.owner_user_id.clone();
    let staged_presentation_id = presentation_id.clone();
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        references.stage_paths(
            &owner_user_id,
            generation,
            &staged_presentation_id,
            epoch,
            paths,
        )
    })
    .await
    .map_err(|_| "参考图读取任务异常退出。".to_owned())?;
    if let Err(error) = authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await
    {
        let _ = manager.references.close_presentation(
            &input.owner_user_id,
            generation,
            &presentation_id,
        );
        return Err(error);
    }
    present_reference_previews(
        &manager,
        &input.owner_user_id,
        generation,
        &presentation_id,
        &preview_session_token,
        &mut result,
    );
    Ok(result)
}

#[tauri::command]
pub(crate) async fn pick_media_studio_video_reference_audio(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioReferenceSelectionInput,
) -> Result<Option<MediaVideoAudioDraft>, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    manager.activate_session(&input.owner_user_id, generation)?;
    let (epoch, presentation_id) = manager.video_inputs.begin_scope(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
    )?;
    let path = rfd::AsyncFileDialog::new()
        .set_title("选择参考音频")
        .add_filter("音频", &["mp3", "wav", "m4a", "aac"])
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf());
    let Some(path) = path else { return Ok(None) };
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let inputs = manager.video_inputs.clone();
    let owner_user_id = input.owner_user_id.clone();
    let staged_presentation_id = presentation_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        inputs.stage_path(
            &owner_user_id,
            generation,
            &staged_presentation_id,
            epoch,
            path,
        )
    })
    .await
    .map_err(|_| "参考音频读取任务异常退出。".to_owned())?;
    if let Err(error) = authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await
    {
        let _ = manager.video_inputs.close_presentation(
            &input.owner_user_id,
            generation,
            &presentation_id,
        );
        return Err(error);
    }
    result.map(Some)
}

#[tauri::command]
pub(crate) async fn stage_media_studio_history_references(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioHistoryReferenceSelectionInput,
) -> Result<MediaImageReferenceImportResult, String> {
    validate_owner(&input.owner_user_id)?;
    if input.assets.len() > MAXIMUM_IMAGE_REFERENCES {
        return Err(format!(
            "参考图最多只能添加 {MAXIMUM_IMAGE_REFERENCES} 张。"
        ));
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let preview_session_token = manager.activate_session(&input.owner_user_id, generation)?;
    let (epoch, presentation_id) =
        manager
            .references
            .begin_scope(&input.owner_user_id, generation, &input.presentation_id)?;
    let history = manager.history.clone();
    let references = manager.references.clone();
    let owner_hash = history::owner_hash(&input.owner_user_id);
    let owner_user_id = input.owner_user_id.clone();
    let command_owner_user_id = input.owner_user_id.clone();
    let staged_presentation_id = presentation_id.clone();
    let assets = input.assets;
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        let mut values = Vec::new();
        let mut errors = Vec::new();
        for asset in assets {
            match history.read_history_asset(&owner_hash, &asset.record_id, &asset.content_token) {
                Ok((_mime_type, bytes)) => {
                    values.push((format!("generated-{}.png", asset.content_token), bytes))
                }
                Err(_) => errors.push("所选生成图片不存在或不属于当前账号。".into()),
            }
        }
        let mut result = references.stage_bytes(
            &owner_user_id,
            generation,
            &staged_presentation_id,
            epoch,
            values,
        );
        result.errors.splice(0..0, errors);
        result
    })
    .await
    .map_err(|_| "生成图片引用任务异常退出。".to_owned())?;
    if let Err(error) = authentication
        .require_session_generation(&command_owner_user_id, generation)
        .await
    {
        let _ = manager.references.close_presentation(
            &command_owner_user_id,
            generation,
            &presentation_id,
        );
        return Err(error);
    }
    present_reference_previews(
        &manager,
        &command_owner_user_id,
        generation,
        &presentation_id,
        &preview_session_token,
        &mut result,
    );
    Ok(result)
}

fn present_reference_previews(
    manager: &MediaStudioManager,
    owner_user_id: &str,
    generation: u64,
    presentation_id: &str,
    preview_session_token: &str,
    result: &mut MediaImageReferenceImportResult,
) {
    for draft in &mut result.references {
        if let Some(token) = manager.references.token_for_reference(
            owner_user_id,
            generation,
            presentation_id,
            &draft.id,
        ) {
            draft.preview_url = Some(media_reference_preview_url(
                preview_session_token,
                presentation_id,
                &token,
            ));
        }
    }
}

#[tauri::command]
pub(crate) async fn discard_media_studio_reference_images(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioReferenceDiscardInput,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    manager.activate_session(&input.owner_user_id, generation)?;
    manager.references.discard(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.reference_ids,
    )
}

#[tauri::command]
pub(crate) async fn discard_media_studio_video_reference_audio(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioVideoAudioDiscardInput,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    manager.video_inputs.discard(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.reference_audio_id,
    )
}

#[tauri::command]
pub(crate) async fn discard_media_studio_presentation(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioReferenceSelectionInput,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let reference_result = manager.references.close_presentation(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
    );
    let video_input_result = manager.video_inputs.close_presentation(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
    );
    let video_job_result = manager.video_jobs.discard_presentation(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
    );
    reference_result
        .and(video_input_result)
        .and(video_job_result)
}

#[tauri::command]
pub(crate) async fn generate_media_studio_image(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    owner_user_id: String,
    request: MediaStudioImageGenerationRequest,
) -> Result<ImageGenerationOutcome, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let preview_session_token = manager.activate_session(&owner_user_id, generation)?;
    let references = manager.references.clone();
    let reference_owner = owner_user_id.clone();
    let reference_presentation = request.presentation_id.clone();
    let reference_ids = request.reference_image_ids.clone();
    let reference_images = tauri::async_runtime::spawn_blocking(move || {
        references.resolve(
            &reference_owner,
            generation,
            &reference_presentation,
            &reference_ids,
        )
    })
    .await
    .map_err(|_| "参考图读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let provider_request_input = ImageGenerationRequest {
        model_config_id: request.model_config_id.clone(),
        prompt: request.prompt.clone(),
        size: request.size.clone(),
        count: request.count,
        reference_images,
        client_request_id: request.client_request_id.clone(),
    };
    let mut models = authentication
        .resolved_models_for_owner(&owner_user_id, vec![request.model_config_id.clone()])
        .await
        .map_err(|error| error.message)?;
    if models.len() != 1 {
        return Err("所选模型已不可用。".into());
    }
    let runtime = resolve_image_model(&owner_user_id, &request.model_config_id, models.remove(0))?;
    authentication.save_model_credential(
        &owner_user_id,
        &runtime.model_config_id,
        &runtime.api_key,
    )?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let provider_request = make_provider_request(&runtime, &provider_request_input)?;
    let response = manager
        .provider_client
        .post(provider_request.url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, provider_request.content_type)
        .bearer_auth(&runtime.api_key)
        .body(provider_request.body)
        .send()
        .await
        .map_err(|_| "无法连接图片模型服务。".to_owned())?;
    let status = response.status();
    let body = read_provider_response(response).await?;
    if !status.is_success() {
        return Err(format!(
            "模型请求失败（HTTP {}）：{}",
            status.as_u16(),
            provider_error_detail(&body, &runtime.api_key)
        ));
    }
    let result = parse_provider_response(&provider_request_input, &runtime, &body)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(persist_generation_result(
        &manager,
        &owner_user_id,
        request.prompt.trim(),
        result,
        &preview_session_token,
    )
    .await)
}

#[tauri::command]
pub(crate) async fn start_media_studio_video_generation(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    owner_user_id: String,
    request: MediaStudioVideoGenerationRequest,
) -> Result<MediaStudioVideoJobSnapshot, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    manager.activate_session(&owner_user_id, generation)?;

    let input_image = if let Some(reference_id) = request.input_image_id.as_deref() {
        let references = manager.references.clone();
        let reference_owner = owner_user_id.clone();
        let presentation_id = request.presentation_id.clone();
        let reference_id = reference_id.to_owned();
        let mut values = tauri::async_runtime::spawn_blocking(move || {
            references.resolve(
                &reference_owner,
                generation,
                &presentation_id,
                &[reference_id],
            )
        })
        .await
        .map_err(|_| "视频首帧读取任务异常退出。".to_owned())??;
        values.pop().map(|value| VideoInput {
            mime_type: value.mime_type,
            bytes: value.bytes,
        })
    } else {
        None
    };
    let reference_audio = request
        .reference_audio_id
        .as_deref()
        .map(|audio_id| {
            manager.video_inputs.resolve(
                &owner_user_id,
                generation,
                &request.presentation_id,
                audio_id,
            )
        })
        .transpose()?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;

    let provider_input = VideoGenerationRequest {
        model_config_id: request.model_config_id.clone(),
        prompt: request.prompt.clone(),
        size: request.size.clone(),
        seconds: request.seconds,
        ratio: request.ratio.clone(),
        input_image,
        last_frame_image: None,
        reference_video: None,
        reference_audio,
        reference_purpose: video_provider::VideoReferencePurpose::Reference,
        client_request_id: request.client_request_id.clone(),
    };
    let mut models = authentication
        .resolved_models_for_owner(&owner_user_id, vec![request.model_config_id.clone()])
        .await
        .map_err(|error| error.message)?;
    if models.len() != 1 {
        return Err("所选模型已不可用。".into());
    }
    let runtime = resolve_video_model(&owner_user_id, &request.model_config_id, models.remove(0))?;
    authentication.save_model_credential(
        &owner_user_id,
        &runtime.model_config_id,
        &runtime.api_key,
    )?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let wire = make_video_create_request(&runtime, &provider_input)?;
    let response = manager
        .provider_client
        .post(wire.url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .bearer_auth(&runtime.api_key)
        .body(wire.body)
        .send()
        .await
        .map_err(|_| "无法连接视频模型服务。".to_owned())?;
    let status = response.status();
    let body = read_bounded_response(
        response,
        MAXIMUM_VIDEO_JOB_RESPONSE_BYTES,
        "无法读取视频模型响应。",
        "模型返回的视频任务数据过大。",
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "创建视频任务失败（HTTP {}）：{}",
            status.as_u16(),
            provider_error_detail(&body, &runtime.api_key)
        ));
    }
    let provider_job = parse_video_job(&body, &runtime.api_key)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    manager.video_jobs.register(
        &owner_user_id,
        generation,
        &request.presentation_id,
        provider_input,
        runtime,
        provider_job,
    )
}

#[tauri::command]
pub(crate) async fn poll_media_studio_video_generation(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioVideoJobInput,
) -> Result<MediaStudioVideoJobSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let preview_session_token = manager.activate_session(&input.owner_user_id, generation)?;
    let start = manager.video_jobs.begin_poll(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.job_id,
    )?;
    let VideoPollStart::Work(work) = start else {
        let VideoPollStart::Snapshot(snapshot) = start else {
            unreachable!()
        };
        return Ok(*snapshot);
    };
    match advance_video_job(
        &authentication,
        &manager,
        &input.owner_user_id,
        generation,
        &preview_session_token,
        &work,
    )
    .await
    {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => manager.video_jobs.fail(&work, error.clone()).or(Err(error)),
    }
}

#[tauri::command]
pub(crate) async fn cancel_media_studio_video_generation(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    input: MediaStudioVideoJobInput,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    manager.video_jobs.cancel(
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.job_id,
    )
}

async fn advance_video_job(
    authentication: &AuthenticationManager,
    manager: &MediaStudioManager,
    owner_user_id: &str,
    generation: u64,
    preview_session_token: &str,
    work: &VideoPollWork,
) -> Result<MediaStudioVideoJobSnapshot, String> {
    if work.cancelled.load(Ordering::Acquire) {
        return Err("视频生成已取消。".into());
    }
    let provider_job = if work.provider_job.should_poll() {
        authentication
            .require_session_generation(owner_user_id, generation)
            .await?;
        let url = video_job_endpoint(&work.runtime, &work.provider_job.id)?;
        let response = manager
            .provider_client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .bearer_auth(&work.runtime.api_key)
            .send()
            .await
            .map_err(|_| "无法查询视频生成任务。".to_owned())?;
        let status = response.status();
        let body = read_bounded_response(
            response,
            MAXIMUM_VIDEO_JOB_RESPONSE_BYTES,
            "无法读取视频任务响应。",
            "模型返回的视频任务数据过大。",
        )
        .await?;
        if !status.is_success() {
            return Err(format!(
                "查询视频任务失败（HTTP {}）：{}",
                status.as_u16(),
                provider_error_detail(&body, &work.runtime.api_key)
            ));
        }
        authentication
            .require_session_generation(owner_user_id, generation)
            .await?;
        if work.cancelled.load(Ordering::Acquire) {
            return Err("视频生成已取消。".into());
        }
        parse_video_job(&body, &work.runtime.api_key)?
    } else {
        work.provider_job.clone()
    };

    if provider_job.is_failed() {
        return Err(format!(
            "视频生成失败：{}",
            provider_job
                .error_message
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("模型未提供失败原因")
        ));
    }
    if provider_job.should_poll() {
        return manager.video_jobs.update_waiting(work, provider_job);
    }
    manager
        .video_jobs
        .begin_download(work, provider_job.clone())?;
    authentication
        .require_session_generation(owner_user_id, generation)
        .await?;
    if work.cancelled.load(Ordering::Acquire) {
        return Err("视频生成已取消。".into());
    }

    let (response, forwards_credential) = if let Some(url) = provider_job.content_url.clone() {
        (
            manager
                .provider_client
                .get(url)
                .header(reqwest::header::ACCEPT, "video/mp4")
                .send()
                .await,
            false,
        )
    } else {
        let url = video_content_endpoint(&work.runtime, &provider_job.id)?;
        (
            manager
                .provider_client
                .get(url)
                .header(reqwest::header::ACCEPT, "video/mp4")
                .bearer_auth(&work.runtime.api_key)
                .send()
                .await,
            true,
        )
    };
    let response = response.map_err(|_| "无法下载生成的视频。".to_owned())?;
    if !forwards_credential && response.url().scheme() != "https" {
        return Err("模型返回的视频下载地址无效。".into());
    }
    let status = response.status();
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("video/mp4")
        .to_owned();
    let body = read_bounded_response(
        response,
        MAXIMUM_VIDEO_BYTES,
        "无法读取生成的视频。",
        "模型返回的视频文件过大。",
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "下载视频内容失败（HTTP {}）：{}",
            status.as_u16(),
            provider_error_detail(
                &body,
                if forwards_credential {
                    &work.runtime.api_key
                } else {
                    ""
                }
            )
        ));
    }
    authentication
        .require_session_generation(owner_user_id, generation)
        .await?;
    if work.cancelled.load(Ordering::Acquire) {
        return Err("视频生成已取消。".into());
    }
    let result = make_video_result(
        &work.request,
        &work.runtime,
        &provider_job,
        &mime_type,
        body,
    )?;
    manager.video_jobs.begin_saving(work)?;
    let history = manager.video_history.clone();
    let owner = owner_user_id.to_owned();
    let prompt = work.request.prompt.trim().to_owned();
    let cancelled = Arc::clone(&work.cancelled);
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        history.persist_video(&owner, &prompt, &result, &cancelled)
    })
    .await
    .map_err(|_| "视频历史保存任务异常退出。".to_owned())?;
    authentication
        .require_session_generation(owner_user_id, generation)
        .await?;
    if work.cancelled.load(Ordering::Acquire) {
        return Err("视频生成已取消。".into());
    }
    match persisted {
        Ok(item) => manager.video_jobs.complete(
            work,
            manager.present_video_history_item(item, preview_session_token),
        ),
        Err(error) => manager
            .video_jobs
            .fail(work, format!("视频已生成，但未能保存到本机：{error}")),
    }
}

async fn persist_generation_result(
    manager: &MediaStudioManager,
    owner_user_id: &str,
    prompt: &str,
    result: provider::ImageGenerationResult,
    preview_session_token: &str,
) -> ImageGenerationOutcome {
    match manager
        .history
        .persist_image(owner_user_id, prompt, &result)
        .await
    {
        Ok(item) => ImageGenerationOutcome::Saved {
            item: manager.present_history_item(item, preview_session_token),
        },
        Err(persistence_error_message) => ImageGenerationOutcome::GeneratedWithoutPersistence {
            result,
            persistence_error_message,
        },
    }
}

pub(crate) fn media_studio_protocol(
    manager: MediaStudioManager,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    manager.protocol_response(request)
}

fn media_preview_url(session_token: &str, record_id: &str, content_token: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("http://chatos-media.localhost/{session_token}/{record_id}/{content_token}")
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("chatos-media://localhost/{session_token}/{record_id}/{content_token}")
    }
}

fn media_reference_preview_url(session_token: &str, presentation_id: &str, token: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("http://chatos-media.localhost/{session_token}/reference/{presentation_id}/{token}")
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("chatos-media://localhost/{session_token}/reference/{presentation_id}/{token}")
    }
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.to_string() == value)
        .unwrap_or(false)
}

fn valid_content_token(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

async fn read_provider_response(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAXIMUM_PROVIDER_RESPONSE_BYTES as u64)
    {
        return Err("模型返回的数据过大。".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "无法读取图片模型响应。".to_owned())?
    {
        if body.len().saturating_add(chunk.len()) > MAXIMUM_PROVIDER_RESPONSE_BYTES {
            return Err("模型返回的数据过大。".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn read_bounded_response(
    mut response: reqwest::Response,
    maximum_bytes: usize,
    read_error: &str,
    too_large_error: &str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum_bytes as u64)
    {
        return Err(too_large_error.into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| read_error.to_owned())? {
        if body.len().saturating_add(chunk.len()) > maximum_bytes {
            return Err(too_large_error.into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn present_model_catalog(
    owner_user_id: &str,
    catalog: Vec<GatewayModelConfig>,
) -> Result<Vec<MediaStudioModelOption>, String> {
    let mut models = catalog
        .into_iter()
        .filter(|model| {
            model.owner_user_id == owner_user_id
                && model.enabled
                && model.has_api_key
                && !model.id.trim().is_empty()
                && !model.name.trim().is_empty()
                && !model.provider.trim().is_empty()
                && !model.model.trim().is_empty()
        })
        .map(|model| MediaStudioModelOption {
            id: model.id,
            name: model.name,
            provider: model.provider,
            model_name: model.model,
            enabled: model.enabled,
            task_enabled: model.task_enabled.unwrap_or(model.enabled),
            has_api_key: model.has_api_key,
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        image_model_rank(left)
            .cmp(&image_model_rank(right))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(models)
}

fn image_model_rank(model: &MediaStudioModelOption) -> u8 {
    if format!("{} {}", model.name, model.model_name)
        .to_lowercase()
        .contains("image")
    {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use provider::{GeneratedImageAsset, ImageGenerationResult};
    use std::fs;
    use tempfile::tempdir;

    fn model(id: &str, owner: &str, name: &str, enabled: bool) -> GatewayModelConfig {
        GatewayModelConfig {
            id: id.into(),
            owner_user_id: owner.into(),
            name: name.into(),
            provider: "openai".into(),
            model: name.into(),
            api_key: Some("must-never-cross".into()),
            base_url: Some("https://provider.example/v1".into()),
            task_thinking_level: None,
            temperature: None,
            max_output_tokens: None,
            enabled,
            task_enabled: Some(false),
            has_api_key: true,
            supports_images: true,
            supports_reasoning: false,
            supports_responses: true,
        }
    }

    #[test]
    fn catalog_matches_macos_filter_sort_and_redacts_secrets() {
        let models = present_model_catalog(
            "alice",
            vec![
                model("chat", "alice", "Zeta", true),
                model("image", "alice", "Image Alpha", true),
                model("disabled", "alice", "Image Disabled", false),
                model("other", "bob", "Image Other", true),
            ],
        )
        .unwrap();
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["image", "chat"]
        );
        let encoded = serde_json::to_string(&models).unwrap();
        assert!(!encoded.contains("must-never-cross"));
        assert!(!encoded.contains("api_key"));
        assert!(encoded.contains("hasApiKey"));
    }

    #[test]
    fn generation_ipc_accepts_only_opaque_reference_ids() {
        let presentation_id = Uuid::new_v4().to_string();
        let valid = serde_json::json!({
            "presentationId": presentation_id,
            "modelConfigId": "image-model",
            "prompt": "quiet lake",
            "size": "1024x1024",
            "count": 1,
            "referenceImageIds": ["reference_123"],
            "clientRequestId": "request-1"
        });
        let request: MediaStudioImageGenerationRequest =
            serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(request.reference_image_ids, vec!["reference_123"]);

        let mut raw_reference = valid;
        raw_reference["referenceImages"] = serde_json::json!([{
            "name": "secret.png",
            "mimeType": "image/png",
            "base64Data": "aW1hZ2U="
        }]);
        assert!(
            serde_json::from_value::<MediaStudioImageGenerationRequest>(raw_reference).is_err()
        );
    }

    #[test]
    fn video_generation_ipc_accepts_only_opaque_native_inputs() {
        let presentation_id = Uuid::new_v4().to_string();
        let valid = serde_json::json!({
            "presentationId": presentation_id,
            "modelConfigId": "video-model",
            "prompt": "running dog",
            "size": "768P",
            "seconds": 4,
            "ratio": "16:9",
            "inputImageId": "reference_123",
            "referenceAudioId": "video_audio_123",
            "clientRequestId": "request-1"
        });
        let request: MediaStudioVideoGenerationRequest =
            serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(request.input_image_id.as_deref(), Some("reference_123"));
        assert_eq!(
            request.reference_audio_id.as_deref(),
            Some("video_audio_123")
        );

        let mut raw_media = valid;
        raw_media["inputImage"] = serde_json::json!({
            "mimeType": "image/png",
            "base64Data": "aW1hZ2U="
        });
        assert!(serde_json::from_value::<MediaStudioVideoGenerationRequest>(raw_media).is_err());
    }

    #[tokio::test]
    async fn persistence_failure_preserves_generated_result_without_native_paths_or_secrets() {
        let root = tempdir().unwrap();
        let app_data = root.path().join("app-data");
        let manager = MediaStudioManager::new(app_data.clone()).unwrap();
        fs::write(
            app_data.join("MediaStudio").join("Accounts"),
            b"not-a-directory",
        )
        .unwrap();
        let preview_session_token = manager.activate_session("alice", 1).unwrap();
        let result = ImageGenerationResult {
            id: "provider-result".into(),
            model_config_id: "image-model".into(),
            model_name: "gpt-image-1".into(),
            created_at_unix_ms: 1_700_000_000_000,
            images: vec![GeneratedImageAsset {
                id: "asset-1".into(),
                mime_type: "image/png".into(),
                base64_data: Some(BASE64.encode(b"generated-image")),
                url: None,
                revised_prompt: None,
            }],
            client_request_id: "request-1".into(),
        };

        let outcome = persist_generation_result(
            &manager,
            "alice",
            "quiet lake",
            result.clone(),
            &preview_session_token,
        )
        .await;
        let ImageGenerationOutcome::GeneratedWithoutPersistence {
            result: preserved,
            persistence_error_message,
        } = &outcome
        else {
            panic!("generation result must survive a persistence failure");
        };
        assert_eq!(preserved, &result);
        assert!(!persistence_error_message.is_empty());

        let encoded = serde_json::to_string(&outcome).unwrap();
        assert!(encoded.contains("generatedWithoutPersistence"));
        assert!(!encoded.contains("api_key"));
        assert!(!encoded.contains("must-never-cross"));
        assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
    }

    #[tokio::test]
    async fn preview_protocol_is_bound_to_the_active_authentication_generation() {
        let root = tempdir().unwrap();
        let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
        let session = manager.activate_session("alice", 7).unwrap();
        let result = ImageGenerationResult {
            id: "provider-result".into(),
            model_config_id: "image-model".into(),
            model_name: "gpt-image-1".into(),
            created_at_unix_ms: 1_700_000_000_000,
            images: vec![GeneratedImageAsset {
                id: "asset-1".into(),
                mime_type: "image/png".into(),
                base64_data: Some(BASE64.encode(b"preview-bytes")),
                url: None,
                revised_prompt: None,
            }],
            client_request_id: "request-1".into(),
        };
        let ImageGenerationOutcome::Saved { item } =
            persist_generation_result(&manager, "alice", "quiet lake", result, &session).await
        else {
            panic!("history should persist");
        };
        let path = format!("/{}/{}/{}", session, item.id, item.images[0].content_token);
        let (mime_type, bytes) = manager.protocol_body(&path).unwrap();
        assert_eq!(mime_type, "image/png");
        assert_eq!(bytes, b"preview-bytes");

        manager.reset();
        assert_eq!(
            manager.protocol_body(&path).unwrap_err(),
            http::StatusCode::UNAUTHORIZED
        );
        let replacement = manager.activate_session("alice", 8).unwrap();
        assert_ne!(session, replacement);
        assert_eq!(
            manager.protocol_body(&path).unwrap_err(),
            http::StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn story_history_bridge_reads_only_the_current_owners_opaque_asset() {
        let root = tempdir().unwrap();
        let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
        let result = ImageGenerationResult {
            id: "provider-result".into(),
            model_config_id: "image-model".into(),
            model_name: "gpt-image-1".into(),
            created_at_unix_ms: 1_700_000_000_000,
            images: vec![GeneratedImageAsset {
                id: "provider-asset".into(),
                mime_type: "image/png".into(),
                base64_data: Some(BASE64.encode(b"owner-scoped-image")),
                url: None,
                revised_prompt: None,
            }],
            client_request_id: "request-1".into(),
        };
        let item = manager
            .history
            .persist_image("alice", "quiet lake", &result)
            .await
            .unwrap();
        let asset = &item.images[0];

        assert_eq!(
            manager
                .read_story_history_image("alice", &item.id, &asset.content_token)
                .unwrap(),
            b"owner-scoped-image"
        );
        assert!(manager
            .read_story_history_image("bob", &item.id, &asset.content_token)
            .is_err());
        assert!(manager
            .read_story_history_image("alice", &item.id, &"0".repeat(32))
            .is_err());
    }

    #[test]
    fn video_preview_uses_the_same_owner_generation_protocol_boundary() {
        use std::sync::atomic::AtomicBool;

        let root = tempdir().unwrap();
        let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
        let session = manager.activate_session("alice", 7).unwrap();
        let item = manager
            .video_history
            .persist_video(
                "alice",
                "clip",
                &video_provider::VideoGenerationResult {
                    id: "provider-video".into(),
                    model_config_id: "video-model".into(),
                    model_name: "MiniMax-H3".into(),
                    created_at_unix_ms: 1_700_000_000_000,
                    mime_type: "video/mp4".into(),
                    video_data: b"video-bytes".to_vec(),
                },
                &AtomicBool::new(false),
            )
            .unwrap();
        let path = format!("/{}/{}/{}", session, item.id, item.content_token);
        let (mime_type, bytes) = manager.protocol_body(&path).unwrap();
        assert_eq!(mime_type, "video/mp4");
        assert_eq!(bytes, b"video-bytes");

        manager.reset();
        assert_eq!(
            manager.protocol_body(&path).unwrap_err(),
            http::StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn reset_clears_video_inputs_and_active_jobs() {
        let root = tempdir().unwrap();
        let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
        manager.activate_session("alice", 7).unwrap();
        let presentation_id = Uuid::new_v4().to_string();
        let (input_epoch, _) = manager
            .video_inputs
            .begin_scope("alice", 7, &presentation_id)
            .unwrap();
        let snapshot = manager
            .video_jobs
            .register(
                "alice",
                7,
                &presentation_id,
                VideoGenerationRequest {
                    model_config_id: "video-model".into(),
                    prompt: "moving clouds".into(),
                    size: "768P".into(),
                    seconds: 4,
                    ratio: "16:9".into(),
                    input_image: None,
                    last_frame_image: None,
                    reference_video: None,
                    reference_audio: None,
                    reference_purpose: video_provider::VideoReferencePurpose::Reference,
                    client_request_id: "request-1".into(),
                },
                video_provider::ResolvedVideoModel {
                    model_config_id: "video-model".into(),
                    model_name: "MiniMax-H3".into(),
                    base_url: reqwest::Url::parse("https://api.example.com/v1/").unwrap(),
                    api_key: "must-never-cross".into(),
                },
                video_provider::ProviderVideoJob {
                    id: "provider-job".into(),
                    status: "queued".into(),
                    progress: None,
                    model_name: Some("MiniMax-H3".into()),
                    error_message: None,
                    content_url: None,
                },
            )
            .unwrap();

        manager.reset();

        let (replacement_epoch, _) = manager
            .video_inputs
            .begin_scope("alice", 7, &presentation_id)
            .unwrap();
        assert_ne!(input_epoch, replacement_epoch);
        assert!(manager
            .video_jobs
            .begin_poll("alice", 7, &presentation_id, &snapshot.job_id)
            .is_err());
    }
}

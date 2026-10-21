#![cfg_attr(not(test), allow(dead_code))]

use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::http;
use uuid::Uuid;

use crate::authentication::GatewayModelConfig;

use super::{
    application::{
        apply_story_project_settings, apply_story_project_source, apply_story_resource_prompt,
        apply_story_segment_edit, make_story_project, StoryImageConfirmationInput,
        StoryLastFrameClearInput, StoryProjectCreationInput, StoryProjectSettingsInput,
        StoryProjectSourceInput, StoryResourcePromptInput, StorySegmentEditInput,
        StorySegmentStructureInput,
    },
    domain::{StoryImageCollection, StoryProject},
    execution::StoryPlanningExecutionRegistry,
    planning::project_fingerprint,
    projection::{StoryMediaPresentation, StoryProjectSnapshot},
    store::{
        StoryProjectCursor, StoryProjectPage, StoryProjectStore, MAXIMUM_STORY_IMAGE_BYTES,
        MAXIMUM_STORY_VIDEO_BYTES,
    },
};

const MAXIMUM_CLOSED_PRESENTATIONS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct StoryMediaScope {
    owner_hash: String,
    authentication_generation: u64,
    presentation_id: String,
}

#[derive(Clone)]
struct StoryMediaEntry {
    scope: StoryMediaScope,
    project_id: String,
    filename: String,
    mime_type: String,
    maximum_bytes: usize,
    token: String,
}

#[derive(Default)]
struct StoryMediaState {
    entries: HashMap<String, StoryMediaEntry>,
    open_presentations: HashSet<StoryMediaScope>,
    closed_presentations: HashSet<StoryMediaScope>,
}

#[derive(Clone, Default)]
struct StoryMediaVault {
    state: Arc<Mutex<StoryMediaState>>,
}

impl StoryMediaVault {
    fn present_project(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        session_token: &str,
        project: &StoryProject,
    ) -> Result<HashMap<String, StoryMediaPresentation>, String> {
        let scope = story_media_scope(owner_user_id, authentication_generation, presentation_id)?;
        if !valid_uuid(session_token) {
            return Err(media_error());
        }
        let specifications = media_specifications(project)?;
        let current_filenames = specifications
            .iter()
            .map(|value| value.filename.as_str())
            .collect::<HashSet<_>>();
        let mut state = self.state.lock().map_err(|_| media_error())?;
        if state.closed_presentations.contains(&scope) {
            return Err("剧情创作页面已关闭，请重新打开后再试。".into());
        }
        state.open_presentations.insert(scope.clone());
        state.entries.retain(|_, entry| {
            entry.scope != scope
                || entry.project_id != project.id
                || current_filenames.contains(entry.filename.as_str())
        });

        let mut presented = HashMap::with_capacity(specifications.len());
        for specification in specifications {
            let existing = state.entries.values().find(|entry| {
                entry.scope == scope
                    && entry.project_id == project.id
                    && entry.filename == specification.filename
                    && entry.mime_type == specification.mime_type
                    && entry.maximum_bytes == specification.maximum_bytes
            });
            let token = existing
                .map(|entry| entry.token.clone())
                .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
            state
                .entries
                .entry(token.clone())
                .or_insert_with(|| StoryMediaEntry {
                    scope: scope.clone(),
                    project_id: project.id.clone(),
                    filename: specification.filename.clone(),
                    mime_type: specification.mime_type,
                    maximum_bytes: specification.maximum_bytes,
                    token: token.clone(),
                });
            presented.insert(
                specification.filename,
                StoryMediaPresentation {
                    content_token: token.clone(),
                    preview_url: story_preview_url(session_token, &scope.presentation_id, &token),
                },
            );
        }
        Ok(presented)
    }

    fn resolve(
        &self,
        owner_hash: &str,
        authentication_generation: u64,
        presentation_id: &str,
        token: &str,
    ) -> Result<StoryMediaEntry, String> {
        if !valid_owner_hash(owner_hash) || !valid_content_token(token) {
            return Err(media_error());
        }
        let presentation_id = canonical_uuid(presentation_id)?;
        let state = self.state.lock().map_err(|_| media_error())?;
        state
            .entries
            .get(token)
            .filter(|entry| {
                entry.scope.owner_hash == owner_hash
                    && entry.scope.authentication_generation == authentication_generation
                    && entry.scope.presentation_id == presentation_id
                    && !state.closed_presentations.contains(&entry.scope)
            })
            .cloned()
            .ok_or_else(media_error)
    }

    fn close_presentation(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(), String> {
        let scope = story_media_scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut state = self.state.lock().map_err(|_| media_error())?;
        state.entries.retain(|_, entry| entry.scope != scope);
        state.open_presentations.remove(&scope);
        state.closed_presentations.insert(scope.clone());
        if state.closed_presentations.len() > MAXIMUM_CLOSED_PRESENTATIONS {
            state.closed_presentations.clear();
            state.closed_presentations.insert(scope);
        }
        Ok(())
    }

    fn require_open_presentation(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(), String> {
        let scope = story_media_scope(owner_user_id, authentication_generation, presentation_id)?;
        let state = self.state.lock().map_err(|_| media_error())?;
        if state.open_presentations.contains(&scope) && !state.closed_presentations.contains(&scope)
        {
            Ok(())
        } else {
            Err("剧情创作页面已经变化，请重新打开后再试。".into())
        }
    }

    fn reset(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.entries.clear();
            state.open_presentations.clear();
            state.closed_presentations.clear();
        }
    }
}

#[derive(Clone)]
struct StoryPreviewSession {
    owner_hash: String,
    authentication_generation: u64,
    token: String,
}

#[derive(Clone)]
pub(crate) struct StoryStudioManager {
    store: StoryProjectStore,
    media: StoryMediaVault,
    preview_session: Arc<Mutex<Option<StoryPreviewSession>>>,
    pub(super) planning_executions: StoryPlanningExecutionRegistry,
}

impl StoryStudioManager {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        Ok(Self {
            store: StoryProjectStore::new(app_data_directory)?,
            media: StoryMediaVault::default(),
            preview_session: Arc::new(Mutex::new(None)),
            planning_executions: StoryPlanningExecutionRegistry::default(),
        })
    }

    pub(crate) fn from_runtime() -> Result<Self, String> {
        let app_data_directory = dirs::data_local_dir()
            .ok_or_else(|| "无法定位应用数据目录。".to_owned())?
            .join("com.chatos.windows");
        Self::new(app_data_directory)
    }

    pub(super) fn present_project(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        project_id: &str,
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        let project = self.store.load_native_project(owner_user_id, project_id)?;
        self.project_snapshot(
            owner_user_id,
            authentication_generation,
            presentation_id,
            &session,
            &project,
        )
    }

    pub(super) fn present_page(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        after: Option<StoryProjectCursor>,
        limit: usize,
    ) -> Result<StoryProjectPage, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        let page = self.store.load_native_page(owner_user_id, after, limit)?;
        let items = page
            .items
            .iter()
            .map(|project| {
                self.project_snapshot(
                    owner_user_id,
                    authentication_generation,
                    presentation_id,
                    &session,
                    project,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(StoryProjectPage {
            items,
            next_cursor: page.next_cursor,
            unreadable_count: page.unreadable_count,
        })
    }

    pub(super) fn create_project(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        input: StoryProjectCreationInput,
        available_models: &[GatewayModelConfig],
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        let project = make_story_project(
            input,
            owner_user_id,
            available_models,
            current_unix_milliseconds(),
        )?;
        self.store.save(owner_user_id, &project)?;
        self.project_snapshot(
            owner_user_id,
            authentication_generation,
            presentation_id,
            &session,
            &project,
        )
    }

    pub(super) fn update_project_settings(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        input: StoryProjectSettingsInput,
        available_models: &[GatewayModelConfig],
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        let project_id = input.project_id.clone();
        let now = current_unix_milliseconds();
        let project = self
            .store
            .mutate_project(owner_user_id, &project_id, |project| {
                apply_story_project_settings(project, input, owner_user_id, available_models, now)
            })?;
        self.project_snapshot(
            owner_user_id,
            authentication_generation,
            presentation_id,
            &session,
            &project,
        )
    }

    pub(super) fn update_project_source(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        input: StoryProjectSourceInput,
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        self.media.require_open_presentation(
            owner_user_id,
            authentication_generation,
            presentation_id,
        )?;
        let project_id = input.project_id.clone();
        let now = current_unix_milliseconds();
        let project = self
            .store
            .mutate_project(owner_user_id, &project_id, |project| {
                apply_story_project_source(project, input, now)
            })?;
        self.project_snapshot(
            owner_user_id,
            authentication_generation,
            presentation_id,
            &session,
            &project,
        )
    }

    pub(super) fn update_resource_prompt(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        input: StoryResourcePromptInput,
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        self.media.require_open_presentation(
            owner_user_id,
            authentication_generation,
            presentation_id,
        )?;
        let project_id = input.project_id.clone();
        let now = current_unix_milliseconds();
        let project = self
            .store
            .mutate_project(owner_user_id, &project_id, |project| {
                apply_story_resource_prompt(project, input, now)
            })?;
        self.project_snapshot(
            owner_user_id,
            authentication_generation,
            presentation_id,
            &session,
            &project,
        )
    }

    pub(super) fn update_segment(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        input: StorySegmentEditInput,
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        self.media.require_open_presentation(
            owner_user_id,
            authentication_generation,
            presentation_id,
        )?;
        let project_id = input.project_id.clone();
        let now = current_unix_milliseconds();
        let project = self
            .store
            .mutate_project(owner_user_id, &project_id, |project| {
                apply_story_segment_edit(project, input, now)
            })?;
        self.project_snapshot(
            owner_user_id,
            authentication_generation,
            presentation_id,
            &session,
            &project,
        )
    }

    pub(super) fn update_segment_structure(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        input: StorySegmentStructureInput,
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner_user_id, authentication_generation)?;
        self.media.require_open_presentation(
            owner_user_id,
            authentication_generation,
            presentation_id,
        )?;
        let project_id = input.project_id.clone();
        let now = current_unix_milliseconds();
        let project = self
            .store
            .mutate_project(owner_user_id, &project_id, |project| {
                super::application::apply_story_segment_structure(project, input, now)
            })?;
        self.project_snapshot(
            owner_user_id,
            authentication_generation,
            presentation_id,
            &session,
            &project,
        )
    }

    pub(super) fn confirm_story_image(
        &self,
        owner: &str,
        generation: u64,
        presentation: &str,
        input: StoryImageConfirmationInput,
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner, generation)?;
        self.media
            .require_open_presentation(owner, generation, presentation)?;
        let project_id = input.project_id.clone();
        let project = self.store.mutate_project(owner, &project_id, |project| {
            super::application::apply_story_image_confirmation(
                project,
                input,
                current_unix_milliseconds(),
            )
        })?;
        self.project_snapshot(owner, generation, presentation, &session, &project)
    }

    pub(super) fn clear_story_last_frame(
        &self,
        owner: &str,
        generation: u64,
        presentation: &str,
        input: StoryLastFrameClearInput,
    ) -> Result<StoryProjectSnapshot, String> {
        let session = self.activate_session(owner, generation)?;
        self.media
            .require_open_presentation(owner, generation, presentation)?;
        let project_id = input.project_id.clone();
        let project = self.store.mutate_project(owner, &project_id, |project| {
            super::application::apply_story_last_frame_clear(
                project,
                input,
                current_unix_milliseconds(),
            )
        })?;
        self.project_snapshot(owner, generation, presentation, &session, &project)
    }

    pub(super) fn discard_presentation(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(), String> {
        self.require_active_session(owner_user_id, authentication_generation, None)?;
        self.planning_executions.cancel_presentation(
            owner_user_id,
            authentication_generation,
            presentation_id,
        );
        self.media
            .close_presentation(owner_user_id, authentication_generation, presentation_id)
    }

    pub(crate) fn reset(&self) {
        self.planning_executions.reset();
        if let Ok(mut session) = self.preview_session.lock() {
            *session = None;
        }
        self.media.reset();
    }

    pub(super) fn reset_session_if_matches(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
    ) {
        let Ok(owner_hash) = owner_hash(owner_user_id) else {
            return;
        };
        if let Ok(mut session) = self.preview_session.lock() {
            let matches = session.as_ref().is_some_and(|active| {
                active.owner_hash == owner_hash
                    && active.authentication_generation == authentication_generation
            });
            if matches {
                self.planning_executions
                    .cancel_session(owner_user_id, authentication_generation);
                *session = None;
                self.media.reset();
            }
        }
    }

    fn project_snapshot(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        session_token: &str,
        project: &StoryProject,
    ) -> Result<StoryProjectSnapshot, String> {
        let presented = self.media.present_project(
            owner_user_id,
            authentication_generation,
            presentation_id,
            session_token,
            project,
        )?;
        Ok(StoryProjectSnapshot::from_project_with_media(
            project,
            Some(project_fingerprint(project)?),
            |filename| presented.get(filename).cloned(),
        ))
    }

    pub(super) fn store(&self) -> StoryProjectStore {
        self.store.clone()
    }

    fn activate_session(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
    ) -> Result<String, String> {
        let owner_hash = owner_hash(owner_user_id)?;
        let mut session = self.preview_session.lock().map_err(|_| media_error())?;
        if let Some(active) = session.as_ref() {
            if active.owner_hash == owner_hash
                && active.authentication_generation == authentication_generation
            {
                return Ok(active.token.clone());
            }
        }
        self.media.reset();
        let token = Uuid::new_v4().to_string();
        *session = Some(StoryPreviewSession {
            owner_hash,
            authentication_generation,
            token: token.clone(),
        });
        Ok(token)
    }

    fn require_active_session(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        session_token: Option<&str>,
    ) -> Result<StoryPreviewSession, String> {
        let owner_hash = owner_hash(owner_user_id)?;
        let session = self.preview_session.lock().map_err(|_| media_error())?;
        session
            .as_ref()
            .filter(|active| {
                active.owner_hash == owner_hash
                    && active.authentication_generation == authentication_generation
                    && session_token.is_none_or(|token| active.token == token)
            })
            .cloned()
            .ok_or_else(media_error)
    }

    fn protocol_response(&self, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
        match self.protocol_body(request.uri().path()) {
            Ok((mime_type, body)) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, mime_type)
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(body)
                .expect("valid story media response"),
            Err(status) => http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"Story media unavailable".to_vec())
                .expect("valid story media error response"),
        }
    }

    fn protocol_body(&self, path: &str) -> Result<(String, Vec<u8>), http::StatusCode> {
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() != 3
            || !valid_uuid(parts[0])
            || !valid_uuid(parts[1])
            || !valid_content_token(parts[2])
        {
            return Err(http::StatusCode::BAD_REQUEST);
        }
        let active = {
            let session = self
                .preview_session
                .lock()
                .map_err(|_| http::StatusCode::SERVICE_UNAVAILABLE)?;
            session
                .as_ref()
                .filter(|active| active.token == parts[0])
                .cloned()
                .ok_or(http::StatusCode::UNAUTHORIZED)?
        };
        let entry = self
            .media
            .resolve(
                &active.owner_hash,
                active.authentication_generation,
                parts[1],
                parts[2],
            )
            .map_err(|_| http::StatusCode::NOT_FOUND)?;
        let bytes = self
            .store
            .read_media_asset(
                &active.owner_hash,
                &entry.project_id,
                &entry.filename,
                &entry.mime_type,
                entry.maximum_bytes,
            )
            .map_err(|_| http::StatusCode::NOT_FOUND)?;
        let still_active = self
            .preview_session
            .lock()
            .ok()
            .and_then(|session| session.clone())
            .is_some_and(|session| session.token == active.token);
        if !still_active {
            return Err(http::StatusCode::UNAUTHORIZED);
        }
        Ok((entry.mime_type, bytes))
    }
}

pub(crate) fn story_studio_protocol(
    manager: StoryStudioManager,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    manager.protocol_response(request)
}

#[derive(Clone)]
struct StoryMediaSpecification {
    filename: String,
    mime_type: String,
    maximum_bytes: usize,
}

fn media_specifications(project: &StoryProject) -> Result<Vec<StoryMediaSpecification>, String> {
    let mut values = Vec::new();
    for collection in project
        .characters
        .iter()
        .map(|value| &value.media)
        .chain(project.scenes.iter().map(|value| &value.media))
        .chain(project.props.iter().map(|value| &value.media))
        .chain(project.segments.iter().map(|value| &value.first_frames))
        .chain(project.segments.iter().map(|value| &value.last_frames))
    {
        append_image_specifications(collection, &mut values);
    }
    for video in project
        .segments
        .iter()
        .flat_map(|segment| segment.video.iter().chain(segment.archived_videos.iter()))
    {
        values.push(StoryMediaSpecification {
            filename: video.filename.clone(),
            mime_type: "video/mp4".into(),
            maximum_bytes: MAXIMUM_STORY_VIDEO_BYTES,
        });
    }
    let mut by_filename = HashMap::<&str, (&str, usize)>::new();
    for value in &values {
        if let Some(existing) = by_filename.insert(
            value.filename.as_str(),
            (value.mime_type.as_str(), value.maximum_bytes),
        ) {
            if existing != (value.mime_type.as_str(), value.maximum_bytes) {
                return Err(media_error());
            }
        }
    }
    values.sort_by(|left, right| left.filename.cmp(&right.filename));
    values.dedup_by(|left, right| left.filename == right.filename);
    Ok(values)
}

fn append_image_specifications(
    collection: &StoryImageCollection,
    values: &mut Vec<StoryMediaSpecification>,
) {
    values.extend(
        collection
            .images
            .iter()
            .map(|image| StoryMediaSpecification {
                filename: image.filename.clone(),
                mime_type: image.mime_type.clone(),
                maximum_bytes: MAXIMUM_STORY_IMAGE_BYTES,
            }),
    );
}

fn story_media_scope(
    owner_user_id: &str,
    authentication_generation: u64,
    presentation_id: &str,
) -> Result<StoryMediaScope, String> {
    Ok(StoryMediaScope {
        owner_hash: owner_hash(owner_user_id)?,
        authentication_generation,
        presentation_id: canonical_uuid(presentation_id)?,
    })
}

fn owner_hash(owner_user_id: &str) -> Result<String, String> {
    let owner = owner_user_id.trim();
    if owner.is_empty() || owner.len() > 1_024 {
        return Err("剧情项目缺少有效的账号上下文。".into());
    }
    Ok(format!("{:x}", Sha256::digest(owner.as_bytes())))
}

fn canonical_uuid(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value).map_err(|_| media_error())?;
    let canonical = parsed.to_string();
    if canonical != value {
        return Err(media_error());
    }
    Ok(canonical)
}

fn valid_uuid(value: &str) -> bool {
    canonical_uuid(value).is_ok()
}

fn valid_owner_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_content_token(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn story_preview_url(session_token: &str, presentation_id: &str, token: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("http://chatos-story.localhost/{session_token}/{presentation_id}/{token}")
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("chatos-story://localhost/{session_token}/{presentation_id}/{token}")
    }
}

fn media_error() -> String {
    "剧情媒体凭据无效或已经过期。".into()
}

fn current_unix_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|value| value.as_millis().try_into().ok())
        .unwrap_or_default()
}

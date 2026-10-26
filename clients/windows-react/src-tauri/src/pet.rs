use chatos_local_agent_protocol::{
    GetTaskGraphCommand, HostCommand, HostResult, ListRunsCommand, ListTaskGraphsCommand,
    LocalAgentRunListScope, LocalAgentRunStatus, LocalAgentRunSummary, LocalTaskGraphListScope,
};
use image::{ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{
    http, AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use uuid::Uuid;

use crate::{
    authentication::AuthenticationManager,
    general_settings::GeneralSettingsStore,
    host_lifecycle::{validate_owner, HostManager},
};

pub(crate) const PET_WINDOW_LABEL: &str = "pet-overlay";
const PET_ACTIVITY_WINDOW_LABEL: &str = "pet-activity-overlay";
const FILE_VERSION: u32 = 1;
const MAXIMUM_FILE_BYTES: u64 = 64 * 1_024;
const MAXIMUM_ATLAS_BYTES: u64 = 20 * 1_024 * 1_024;
const ATLAS_WIDTH: u32 = 8 * 192;
const ATLAS_HEIGHT: u32 = 11 * 208;
const ACTIVITY_WIDTH: f64 = 310.0;
const ACTIVITY_HEIGHT: f64 = 112.0;
const RECENT_TERMINAL_MILLISECONDS: i64 = 15 * 60 * 1_000;

#[derive(Clone)]
pub(crate) struct PetPreferencesStore {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

#[derive(Clone, Default)]
pub(crate) struct PetAssetStore {
    atlas: Arc<Mutex<Option<Vec<u8>>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PetPreferencesSnapshot {
    is_enabled: bool,
    size: u16,
    show_process: bool,
    show_completions: bool,
    show_across_spaces: bool,
    revision: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavedPosition {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PetPreferencesFile {
    version: u32,
    is_enabled: bool,
    size: u16,
    show_process: bool,
    show_completions: bool,
    show_across_spaces: bool,
    revision: u64,
    position: Option<SavedPosition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdatePetPreferencesInput {
    owner_user_id: String,
    is_enabled: bool,
    size: u16,
    show_process: bool,
    show_completions: bool,
    show_across_spaces: bool,
    expected_revision: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PetPresentationProjection {
    language: String,
    animation_state: &'static str,
    primary_activity: Option<PetActivityProjection>,
    active_work_count: u32,
    attention_count: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PetActivityProjection {
    source: &'static str,
    kind: &'static str,
    detail_code: Option<&'static str>,
    updated_at_unix_ms: i64,
}

impl Default for PetPreferencesFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            is_enabled: true,
            size: 104,
            show_process: true,
            show_completions: true,
            show_across_spaces: true,
            revision: 1,
            position: None,
        }
    }
}

impl PetPreferencesFile {
    fn snapshot(&self) -> PetPreferencesSnapshot {
        PetPreferencesSnapshot {
            is_enabled: self.is_enabled,
            size: self.size,
            show_process: self.show_process,
            show_completions: self.show_completions,
            show_across_spaces: self.show_across_spaces,
            revision: self.revision,
        }
    }
}

impl PetPreferencesStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            path: app_data_directory.join("pet").join("preferences.json"),
            lock: Arc::new(Mutex::new(())),
        }
    }

    fn state(&self) -> Result<PetPreferencesFile, String> {
        let _guard = self.lock.lock().map_err(|_| pet_error())?;
        self.load()
    }

    fn update(&self, input: UpdatePetPreferencesInput) -> Result<PetPreferencesFile, String> {
        validate_size(input.size)?;
        if input.expected_revision == 0 {
            return Err("宠物设置版本无效。".into());
        }
        let _guard = self.lock.lock().map_err(|_| pet_error())?;
        let mut file = self.load()?;
        if file.revision != input.expected_revision {
            return Err("宠物设置已经变化，请刷新后重试。".into());
        }
        file.is_enabled = input.is_enabled;
        file.size = input.size;
        file.show_process = input.show_process;
        file.show_completions = input.show_completions;
        file.show_across_spaces = input.show_across_spaces;
        file.revision = file.revision.checked_add(1).ok_or_else(pet_error)?;
        self.save(&file)?;
        Ok(file)
    }

    fn reset_position(&self, expected_revision: u64) -> Result<PetPreferencesFile, String> {
        if expected_revision == 0 {
            return Err("宠物设置版本无效。".into());
        }
        let _guard = self.lock.lock().map_err(|_| pet_error())?;
        let mut file = self.load()?;
        if file.revision != expected_revision {
            return Err("宠物设置已经变化，请刷新后重试。".into());
        }
        file.position = None;
        file.revision = file.revision.checked_add(1).ok_or_else(pet_error)?;
        self.save(&file)?;
        Ok(file)
    }

    fn save_position(&self, position: PhysicalPosition<i32>) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|_| pet_error())?;
        let mut file = self.load()?;
        file.position = Some(SavedPosition {
            x: position.x,
            y: position.y,
        });
        self.save(&file)
    }

    fn load(&self) -> Result<PetPreferencesFile, String> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PetPreferencesFile::default())
            }
            Err(_) => return Err(pet_error()),
        };
        let metadata = file.metadata().map_err(|_| pet_error())?;
        if !metadata.is_file() || metadata.len() > MAXIMUM_FILE_BYTES {
            return Err(pet_error());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAXIMUM_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| pet_error())?;
        if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
            return Err(pet_error());
        }
        let parsed: PetPreferencesFile = serde_json::from_slice(&bytes).map_err(|_| pet_error())?;
        validate_file(&parsed)?;
        Ok(parsed)
    }

    fn save(&self, file: &PetPreferencesFile) -> Result<(), String> {
        validate_file(file)?;
        let bytes = serde_json::to_vec(file).map_err(|_| pet_error())?;
        if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
            return Err(pet_error());
        }
        let parent = self.path.parent().ok_or_else(pet_error)?;
        fs::create_dir_all(parent).map_err(|_| pet_error())?;
        let temporary = parent.join(format!(".preferences-{}.tmp", Uuid::new_v4()));
        let result =
            write_private_file(&temporary, &bytes).and_then(|_| fs::rename(&temporary, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(pet_error());
        }
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    }
}

impl PetAssetStore {
    pub(crate) fn load(&self, resource_directory: &Path) -> Result<(), String> {
        let installed = resource_directory
            .join("Pets")
            .join("fengtuan")
            .join("spritesheet.webp");
        let development = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../macos/Support/Pets/fengtuan/spritesheet.webp");
        let path = if installed.is_file() {
            installed
        } else {
            development
        };
        let bytes = read_bounded(&path, MAXIMUM_ATLAS_BYTES).map_err(|_| pet_asset_error())?;
        validate_atlas(&bytes)?;
        *self.atlas.lock().map_err(|_| pet_asset_error())? = Some(bytes);
        Ok(())
    }

    fn protocol_response(&self, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
        let valid_request = request.method() == http::Method::GET
            && request.uri().path().trim_matches('/') == "fengtuan-atlas";
        let bytes = valid_request
            .then(|| self.atlas.lock().ok()?.clone())
            .flatten();
        match bytes {
            Some(bytes) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "image/webp")
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "public, max-age=31536000, immutable")
                .body(bytes)
                .expect("valid pet asset response"),
            None => http::Response::builder()
                .status(http::StatusCode::NOT_FOUND)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"Pet asset unavailable".to_vec())
                .expect("valid pet asset error response"),
        }
    }
}

#[tauri::command]
pub(crate) async fn load_pet_preferences(
    owner_user_id: String,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, PetPreferencesStore>,
) -> Result<PetPreferencesSnapshot, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = store.inner().clone();
    let state = tauri::async_runtime::spawn_blocking(move || store.state())
        .await
        .map_err(|_| pet_error())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    reconcile_window(&app, &state)?;
    Ok(state.snapshot())
}

#[tauri::command]
pub(crate) async fn update_pet_preferences(
    input: UpdatePetPreferencesInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, PetPreferencesStore>,
) -> Result<PetPreferencesSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    validate_size(input.size)?;
    let owner_user_id = input.owner_user_id.clone();
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let store = store.inner().clone();
    let state = tauri::async_runtime::spawn_blocking(move || store.update(input))
        .await
        .map_err(|_| pet_error())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    reconcile_window(&app, &state)?;
    Ok(state.snapshot())
}

#[tauri::command]
pub(crate) async fn reset_pet_position(
    owner_user_id: String,
    expected_revision: u64,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, PetPreferencesStore>,
) -> Result<PetPreferencesSnapshot, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = store.inner().clone();
    let state =
        tauri::async_runtime::spawn_blocking(move || store.reset_position(expected_revision))
            .await
            .map_err(|_| pet_error())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    reconcile_window(&app, &state)?;
    Ok(state.snapshot())
}

#[tauri::command]
pub(crate) async fn refresh_pet_presentation(
    owner_user_id: String,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    preferences: State<'_, PetPreferencesStore>,
    general_settings: State<'_, GeneralSettingsStore>,
    manager: State<'_, HostManager>,
) -> Result<PetPresentationProjection, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let preference_store = preferences.inner().clone();
    let preference_state = tauri::async_runtime::spawn_blocking(move || preference_store.state())
        .await
        .map_err(|_| pet_error())??;
    let general_store = general_settings.inner().clone();
    let language_owner = owner_user_id.clone();
    let language = tauri::async_runtime::spawn_blocking(move || {
        general_store.interface_language(&language_owner)
    })
    .await
    .map_err(|_| pet_error())??;
    if !preference_state.is_enabled {
        let idle = idle_presentation(&language);
        publish_presentation(&app, &idle);
        return Ok(idle);
    }
    let presentation = match fetch_presentation(
        &manager,
        &owner_user_id,
        &preference_state,
        &language,
        chrono::Utc::now().timestamp_millis(),
    )
    .await
    {
        Ok(presentation) => presentation,
        Err(error) => {
            publish_presentation(&app, &idle_presentation(&language));
            return Err(error);
        }
    };
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    publish_presentation(&app, &presentation);
    Ok(presentation)
}

pub(crate) fn pet_asset_protocol(
    assets: PetAssetStore,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    assets.protocol_response(request)
}

pub(crate) fn hide_pet_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PET_WINDOW_LABEL) {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window(PET_ACTIVITY_WINDOW_LABEL) {
        let _ = window.hide();
    }
}

fn reconcile_window(app: &AppHandle, state: &PetPreferencesFile) -> Result<(), String> {
    if !state.is_enabled {
        hide_pet_overlay(app);
        return Ok(());
    }
    let window = if let Some(window) = app.get_webview_window(PET_WINDOW_LABEL) {
        window
    } else {
        let url = WebviewUrl::App("index.html?surface=pet-overlay".into());
        let window = WebviewWindowBuilder::new(app, PET_WINDOW_LABEL, url)
            .title("ChatOS Pet")
            .inner_size(f64::from(state.size), f64::from(state.size))
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .focused(false)
            .decorations(false)
            .transparent(true)
            .no_redirection_bitmap(true)
            .shadow(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .build()
            .map_err(|_| "无法创建宠物浮窗。".to_owned())?;
        let position_store = app.state::<PetPreferencesStore>().inner().clone();
        let activity_app = app.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::Moved(position) = event {
                let _ = position_store.save_position(*position);
                position_activity_window(&activity_app, *position);
            }
        });
        window
    };
    ensure_activity_window(app)?;
    window
        .set_size(LogicalSize::new(
            f64::from(state.size),
            f64::from(state.size),
        ))
        .map_err(|_| "无法更新宠物浮窗大小。".to_owned())?;
    window
        .set_always_on_top(true)
        .map_err(|_| "无法更新宠物浮窗层级。".to_owned())?;
    #[cfg(not(target_os = "windows"))]
    {
        window
            .set_visible_on_all_workspaces(state.show_across_spaces)
            .map_err(|_| "无法更新宠物浮窗桌面范围。".to_owned())?;
        if let Some(activity) = app.get_webview_window(PET_ACTIVITY_WINDOW_LABEL) {
            activity
                .set_visible_on_all_workspaces(state.show_across_spaces)
                .map_err(|_| "无法更新宠物事件浮窗桌面范围。".to_owned())?;
        }
    }
    let position = match state.position {
        Some(position) => PhysicalPosition::new(position.x, position.y),
        None => default_position(&window, state.size)?,
    };
    window
        .set_position(position)
        .map_err(|_| "无法恢复宠物浮窗位置。".to_owned())?;
    window.show().map_err(|_| "无法显示宠物浮窗。".to_owned())
}

fn ensure_activity_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(PET_ACTIVITY_WINDOW_LABEL).is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        PET_ACTIVITY_WINDOW_LABEL,
        WebviewUrl::App("index.html?surface=pet-activity".into()),
    )
    .title("ChatOS Pet Activity")
    .inner_size(ACTIVITY_WIDTH, ACTIVITY_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .focused(false)
    .decorations(false)
    .transparent(true)
    .no_redirection_bitmap(true)
    .shadow(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible(false)
    .build()
    .map(|_| ())
    .map_err(|_| "无法创建宠物事件浮窗。".to_owned())
}

fn position_activity_window(app: &AppHandle, pet_position: PhysicalPosition<i32>) {
    let Some(pet) = app.get_webview_window(PET_WINDOW_LABEL) else {
        return;
    };
    let Some(activity) = app.get_webview_window(PET_ACTIVITY_WINDOW_LABEL) else {
        return;
    };
    let Ok(pet_size) = pet.outer_size() else {
        return;
    };
    let Some(monitor) = pet
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| pet.primary_monitor().ok().flatten())
    else {
        return;
    };
    let scale = monitor.scale_factor();
    let activity_width = (ACTIVITY_WIDTH * scale).round() as i32;
    let activity_height = (ACTIVITY_HEIGHT * scale).round() as i32;
    let pet_width = i32::try_from(pet_size.width).unwrap_or_default();
    let pet_height = i32::try_from(pet_size.height).unwrap_or_default();
    let area = monitor.work_area();
    let area_width = i32::try_from(area.size.width).unwrap_or_default();
    let area_height = i32::try_from(area.size.height).unwrap_or_default();
    let minimum_x = area.position.x + 8;
    let maximum_x = area.position.x + area_width - activity_width - 8;
    let centered_x = pet_position.x + (pet_width - activity_width) / 2;
    let x = if maximum_x >= minimum_x {
        centered_x.clamp(minimum_x, maximum_x)
    } else {
        area.position.x
    };
    let gap = (10.0 * scale).round() as i32;
    let above = pet_position.y - activity_height - gap;
    let y = if above >= area.position.y + 8 {
        above
    } else {
        (pet_position.y + pet_height + gap).min(area.position.y + area_height - activity_height - 8)
    };
    let _ = activity.set_position(PhysicalPosition::new(x, y));
}

fn publish_presentation(app: &AppHandle, presentation: &PetPresentationProjection) {
    if let Some(window) = app.get_webview_window(PET_WINDOW_LABEL) {
        let _ = window.emit("pet-presentation", presentation);
        if let Ok(position) = window.outer_position() {
            position_activity_window(app, position);
        }
    }
    if let Some(window) = app.get_webview_window(PET_ACTIVITY_WINDOW_LABEL) {
        let _ = window.emit("pet-presentation", presentation);
        if presentation.primary_activity.is_some() {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    }
}

async fn fetch_presentation(
    manager: &HostManager,
    owner_user_id: &str,
    preferences: &PetPreferencesFile,
    language: &str,
    now_unix_ms: i64,
) -> Result<PetPresentationProjection, String> {
    let active = list_runs(manager, owner_user_id, LocalAgentRunListScope::Active, None).await?;
    let task_display_statuses = task_display_statuses(manager, owner_user_id, &active).await;
    let cutoff = now_unix_ms.saturating_sub(RECENT_TERMINAL_MILLISECONDS);
    let terminal = list_runs(
        manager,
        owner_user_id,
        LocalAgentRunListScope::Terminal,
        Some(cutoff),
    )
    .await?;
    project_presentation(
        owner_user_id,
        active,
        terminal,
        &task_display_statuses,
        preferences,
        language,
        now_unix_ms,
    )
}

async fn task_display_statuses(
    manager: &HostManager,
    owner_user_id: &str,
    runs: &[LocalAgentRunSummary],
) -> BTreeMap<String, String> {
    let mut unresolved = runs
        .iter()
        .filter(|run| run.owner_entity_type == "task")
        .map(|run| run.owner_entity_id.clone())
        .collect::<BTreeSet<_>>();
    if unresolved.is_empty() {
        return BTreeMap::new();
    }
    let Ok(result) = manager
        .execute(HostCommand::ListTaskGraphs(ListTaskGraphsCommand {
            owner_user_id: owner_user_id.to_owned(),
            scope: LocalTaskGraphListScope::Active,
            source_entity_type: None,
            source_entity_id: None,
            before_updated_at_unix_ms: None,
            before_graph_id: None,
            limit: 100,
        }))
        .await
    else {
        return BTreeMap::new();
    };
    let HostResult::TaskGraphs { page } = result else {
        return BTreeMap::new();
    };
    let mut statuses = BTreeMap::new();
    for summary in page.graphs {
        if unresolved.is_empty() {
            break;
        }
        let Ok(result) = manager
            .execute(HostCommand::GetTaskGraph(GetTaskGraphCommand {
                owner_user_id: owner_user_id.to_owned(),
                graph_id: summary.graph_id.clone(),
            }))
            .await
        else {
            continue;
        };
        let HostResult::TaskGraph { graph } = result else {
            continue;
        };
        if graph.owner_user_id != owner_user_id || graph.graph_id != summary.graph_id {
            continue;
        }
        for task in graph.tasks {
            if unresolved.remove(&task.task_id) {
                statuses.insert(
                    task.task_id.clone(),
                    crate::run_inspector::display_status(&task),
                );
            }
        }
    }
    statuses
}

async fn list_runs(
    manager: &HostManager,
    owner_user_id: &str,
    scope: LocalAgentRunListScope,
    updated_after_unix_ms: Option<i64>,
) -> Result<Vec<LocalAgentRunSummary>, String> {
    let result = manager
        .execute(HostCommand::ListRuns(ListRunsCommand {
            owner_user_id: owner_user_id.to_owned(),
            scope,
            status: None,
            updated_after_unix_ms,
            before_updated_at_unix_ms: None,
            before_run_id: None,
            limit: 100,
        }))
        .await?;
    let HostResult::Runs { page } = result else {
        return Err("Local Agent Host 返回了错误的宠物活动列表。".into());
    };
    Ok(page.runs)
}

fn project_presentation(
    owner_user_id: &str,
    active: Vec<LocalAgentRunSummary>,
    terminal: Vec<LocalAgentRunSummary>,
    task_display_statuses: &BTreeMap<String, String>,
    preferences: &PetPreferencesFile,
    language: &str,
    now_unix_ms: i64,
) -> Result<PetPresentationProjection, String> {
    let cutoff = now_unix_ms.saturating_sub(RECENT_TERMINAL_MILLISECONDS);
    let mut runs = BTreeMap::new();
    for run in active.into_iter().chain(terminal) {
        if run.owner_user_id != owner_user_id
            || run.updated_at_unix_ms < 0
            || run.created_at_unix_ms < 0
            || run.version == 0
        {
            return Err("Local Agent Host 返回了无效的宠物活动。".into());
        }
        if run.status.is_terminal() && run.updated_at_unix_ms < cutoff {
            continue;
        }
        runs.entry(run.run_id.clone()).or_insert(run);
    }
    let mut activities = runs
        .into_values()
        .map(|run| {
            let display_status = (run.owner_entity_type == "task")
                .then(|| task_display_statuses.get(&run.owner_entity_id))
                .flatten()
                .map(String::as_str);
            project_activity(run, display_status)
        })
        .filter(|activity| preferences.show_process || activity.kind != "working")
        .filter(|activity| preferences.show_completions || activity.kind != "succeeded")
        .collect::<Vec<_>>();
    activities.sort_by(|left, right| {
        activity_priority(right.kind)
            .cmp(&activity_priority(left.kind))
            .then_with(|| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms))
    });
    let active_work_count = activities
        .iter()
        .filter(|activity| activity.kind == "working")
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let attention_count = activities
        .iter()
        .filter(|activity| matches!(activity.kind, "waiting_for_user" | "reviewing" | "failed"))
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let primary_activity = activities.into_iter().next();
    let animation_state = primary_activity
        .as_ref()
        .map(|activity| animation_state(activity.kind))
        .unwrap_or("idle");
    Ok(PetPresentationProjection {
        language: language.to_owned(),
        animation_state,
        primary_activity,
        active_work_count,
        attention_count,
    })
}

fn project_activity(
    run: LocalAgentRunSummary,
    task_display_status: Option<&str>,
) -> PetActivityProjection {
    let task_gate = match task_display_status {
        Some("reviewing_followup") => Some("reviewing_followup"),
        Some("waiting_confirmation") => Some("waiting_confirmation"),
        _ => None,
    };
    let kind = match run.status {
        LocalAgentRunStatus::WaitingUser => "waiting_for_user",
        LocalAgentRunStatus::Paused | LocalAgentRunStatus::NeedsReview => "reviewing",
        LocalAgentRunStatus::Succeeded => "succeeded",
        LocalAgentRunStatus::Failed => "failed",
        LocalAgentRunStatus::Cancelled => "cancelled",
        _ => "working",
    };
    let detail_code = task_gate.or(match run.status {
        LocalAgentRunStatus::WaitingUser => Some("waiting_user"),
        LocalAgentRunStatus::WaitingToolResult => Some("waiting_tool"),
        LocalAgentRunStatus::RetryScheduled => Some("retry_scheduled"),
        LocalAgentRunStatus::Paused | LocalAgentRunStatus::NeedsReview => Some("needs_review"),
        _ => None,
    });
    PetActivityProjection {
        source: if run.profile_key == "main_chat" {
            "chat"
        } else {
            "task_execution"
        },
        kind,
        detail_code,
        updated_at_unix_ms: run.updated_at_unix_ms,
    }
}

fn activity_priority(kind: &str) -> u8 {
    match kind {
        "waiting_for_user" => 5,
        "failed" => 4,
        "succeeded" => 3,
        "reviewing" => 2,
        "working" => 1,
        _ => 0,
    }
}

fn animation_state(kind: &str) -> &'static str {
    match kind {
        "working" => "running",
        "reviewing" => "review",
        "waiting_for_user" => "waiting",
        "succeeded" => "succeeded",
        "failed" => "failed",
        _ => "idle",
    }
}

fn idle_presentation(language: &str) -> PetPresentationProjection {
    PetPresentationProjection {
        language: language.to_owned(),
        animation_state: "idle",
        primary_activity: None,
        active_work_count: 0,
        attention_count: 0,
    }
}

fn default_position(
    window: &tauri::WebviewWindow,
    logical_size: u16,
) -> Result<PhysicalPosition<i32>, String> {
    let monitor = window
        .primary_monitor()
        .map_err(|_| "无法读取主屏幕信息。".to_owned())?
        .ok_or_else(|| "没有可用的主屏幕。".to_owned())?;
    let area = monitor.work_area();
    let physical_size = (f64::from(logical_size) * monitor.scale_factor()).round() as i32;
    let width = i32::try_from(area.size.width).map_err(|_| "主屏幕尺寸无效。".to_owned())?;
    let height = i32::try_from(area.size.height).map_err(|_| "主屏幕尺寸无效。".to_owned())?;
    Ok(PhysicalPosition::new(
        area.position.x + width - physical_size - 30,
        area.position.y + height - physical_size - 48,
    ))
}

fn validate_file(file: &PetPreferencesFile) -> Result<(), String> {
    if file.version != FILE_VERSION || file.revision == 0 {
        return Err(pet_error());
    }
    validate_size(file.size)
}

fn validate_size(size: u16) -> Result<(), String> {
    if (72..=180).contains(&size) && (size - 72).is_multiple_of(4) {
        Ok(())
    } else {
        Err("宠物大小必须在 72 到 180 之间，并以 4 为步长。".into())
    }
}

fn validate_atlas(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() as u64 > MAXIMUM_ATLAS_BYTES {
        return Err(pet_asset_error());
    }
    let dimensions = ImageReader::with_format(Cursor::new(bytes), ImageFormat::WebP)
        .into_dimensions()
        .map_err(|_| pet_asset_error())?;
    if dimensions != (ATLAS_WIDTH, ATLAS_HEIGHT) {
        return Err(pet_asset_error());
    }
    Ok(())
}

fn read_bounded(path: &Path, maximum_bytes: u64) -> std::io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum_bytes {
        return Err(std::io::Error::other("invalid pet asset"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(std::io::Error::other("pet asset too large"));
    }
    Ok(bytes)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn pet_error() -> String {
    "无法读取或保存宠物偏好。".into()
}

fn pet_asset_error() -> String {
    "宠物资源无效或不可用。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preferences_are_device_wide_and_revision_guarded() {
        let directory = tempfile::tempdir().unwrap();
        let store = PetPreferencesStore::new(directory.path().to_path_buf());
        let initial = store.state().unwrap();
        assert_eq!(initial.snapshot(), PetPreferencesFile::default().snapshot());
        let updated = store
            .update(UpdatePetPreferencesInput {
                owner_user_id: "alice".into(),
                is_enabled: false,
                size: 180,
                show_process: false,
                show_completions: true,
                show_across_spaces: false,
                expected_revision: 1,
            })
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(store.state().unwrap(), updated);
        assert!(store
            .update(UpdatePetPreferencesInput {
                owner_user_id: "bob".into(),
                is_enabled: true,
                size: 104,
                show_process: true,
                show_completions: true,
                show_across_spaces: true,
                expected_revision: 1,
            })
            .unwrap_err()
            .contains("已经变化"));
    }

    #[test]
    fn size_requires_the_macos_range_and_step() {
        for valid in [72, 76, 104, 180] {
            assert!(validate_size(valid).is_ok());
        }
        for invalid in [0, 71, 73, 178, 181] {
            assert!(validate_size(invalid).is_err());
        }
    }

    #[test]
    fn position_updates_do_not_advance_the_settings_revision() {
        let directory = tempfile::tempdir().unwrap();
        let store = PetPreferencesStore::new(directory.path().to_path_buf());
        store
            .save_position(PhysicalPosition::new(-120, 480))
            .unwrap();
        let saved = store.state().unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.position, Some(SavedPosition { x: -120, y: 480 }));
    }

    #[test]
    fn bundled_fengtuan_atlas_matches_the_macos_contract() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../macos/Support/Pets/fengtuan/spritesheet.webp");
        let bytes = read_bounded(&path, MAXIMUM_ATLAS_BYTES).unwrap();
        validate_atlas(&bytes).unwrap();
    }

    #[test]
    fn run_projection_matches_macos_priority_and_preference_filters() {
        let now = 2_000_000;
        let active = vec![
            run("working", LocalAgentRunStatus::ModelRunning, now - 10),
            run("waiting", LocalAgentRunStatus::WaitingUser, now - 20),
        ];
        let terminal = vec![run("done", LocalAgentRunStatus::Succeeded, now - 30)];
        let presentation = project_presentation(
            "alice",
            active.clone(),
            terminal.clone(),
            &BTreeMap::new(),
            &PetPreferencesFile::default(),
            "zh-CN",
            now,
        )
        .unwrap();
        assert_eq!(presentation.animation_state, "waiting");
        assert_eq!(
            presentation.primary_activity.unwrap().kind,
            "waiting_for_user"
        );
        assert_eq!(presentation.active_work_count, 1);

        let preferences = PetPreferencesFile {
            show_process: false,
            show_completions: false,
            ..PetPreferencesFile::default()
        };
        let filtered = project_presentation(
            "alice",
            active,
            terminal,
            &BTreeMap::new(),
            &preferences,
            "zh-CN",
            now,
        )
        .unwrap();
        assert_eq!(filtered.primary_activity.unwrap().kind, "waiting_for_user");
        assert_eq!(filtered.active_work_count, 0);
    }

    #[test]
    fn run_projection_rejects_cross_owner_and_expires_old_terminal_runs() {
        let now = 2_000_000;
        let mut foreign = run("foreign", LocalAgentRunStatus::ModelRunning, now);
        foreign.owner_user_id = "bob".into();
        assert!(project_presentation(
            "alice",
            vec![foreign],
            vec![],
            &BTreeMap::new(),
            &PetPreferencesFile::default(),
            "zh-CN",
            now,
        )
        .is_err());

        let expired = run(
            "expired",
            LocalAgentRunStatus::Failed,
            now - RECENT_TERMINAL_MILLISECONDS - 1,
        );
        let presentation = project_presentation(
            "alice",
            vec![],
            vec![expired],
            &BTreeMap::new(),
            &PetPreferencesFile::default(),
            "zh-CN",
            now,
        )
        .unwrap();
        assert_eq!(presentation, idle_presentation("zh-CN"));
    }

    #[test]
    fn task_followup_gate_overrides_stale_running_pet_activity() {
        let now = 2_000_000;
        for gate in ["reviewing_followup", "waiting_confirmation"] {
            let run = run("gated", LocalAgentRunStatus::ModelRunning, now);
            let task_id = run.owner_entity_id.clone();
            let presentation = project_presentation(
                "alice",
                vec![run],
                vec![],
                &BTreeMap::from([(task_id, gate.to_owned())]),
                &PetPreferencesFile::default(),
                "zh-CN",
                now,
            )
            .unwrap();
            assert_eq!(presentation.animation_state, "running");
            assert_eq!(presentation.active_work_count, 1);
            assert_eq!(presentation.attention_count, 0);
            let activity = presentation.primary_activity.unwrap();
            assert_eq!(activity.kind, "working");
            assert_eq!(activity.detail_code, Some(gate));
        }
    }

    fn run(id: &str, status: LocalAgentRunStatus, updated_at_unix_ms: i64) -> LocalAgentRunSummary {
        LocalAgentRunSummary {
            run_id: id.into(),
            owner_user_id: "alice".into(),
            owner_entity_type: "task".into(),
            owner_entity_id: format!("task-{id}"),
            profile_key: "task_execution".into(),
            capability_policy_revision: "policy-1".into(),
            input: json!({"private": "never projected"}),
            status,
            version: 1,
            terminal_outcome: Some(json!({"secret": "never projected"})),
            created_at_unix_ms: updated_at_unix_ms.saturating_sub(100),
            updated_at_unix_ms,
        }
    }
}

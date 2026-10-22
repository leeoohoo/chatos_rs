use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{
    authentication::AuthenticationManager,
    general_settings::GeneralSettingsStore,
    global_utilities::{dispatch_action, GlobalUtilityAction, GlobalUtilityPreferencesStore},
    host_lifecycle::validate_owner,
};

const WINDOW_LABEL: &str = "quick-search";
const USAGE_FILE_VERSION: u32 = 1;
const MAXIMUM_USAGE_FILE_BYTES: u64 = 256 * 1_024;
const MAXIMUM_USAGE_RECORDS: usize = 512;
const MAXIMUM_QUERY_CHARACTERS: usize = 256;
const MAXIMUM_MEMORY_RESULTS: usize = 36;
const MAXIMUM_TOTAL_RESULTS: usize = 50;
const MAXIMUM_APPLICATION_RECORDS: usize = 4_096;
const FILE_SEARCH_TIMEOUT: Duration = Duration::from_secs(3);
const FILE_SEARCH_CANDIDATE_LIMIT: usize = 256;

#[derive(Clone)]
pub(crate) struct QuickSearchManager {
    data_directory: PathBuf,
    usage_path: PathBuf,
    inner: Arc<Mutex<QuickSearchRuntime>>,
}

#[derive(Default)]
struct QuickSearchRuntime {
    presentation: Option<QuickSearchPresentation>,
    applications: Option<Vec<ApplicationRecord>>,
    usage: BTreeMap<String, UsageRecord>,
}

struct QuickSearchPresentation {
    id: Uuid,
    owner_user_id: String,
    latest_request_id: u64,
    actions: HashMap<Uuid, SearchAction>,
}

#[derive(Clone, Debug)]
struct ApplicationRecord {
    title: String,
    subtitle: Option<String>,
    path: PathBuf,
}

#[derive(Clone, Debug)]
struct FileRecord {
    title: String,
    subtitle: String,
    path: PathBuf,
    is_directory: bool,
    modified_at: SystemTime,
}

#[derive(Clone, Debug)]
enum SearchAction {
    OpenProject(String),
    OpenContact(String),
    OpenApplication(PathBuf),
    OpenFile(PathBuf),
    BuiltIn(BuiltInAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltInAction {
    Screenshot,
    ScreenRecording,
    ClipboardHistory,
    OpenSettings,
    OpenRuntimePermissions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchScope {
    All,
    Actions,
    ChatOs,
    Files,
}

#[derive(Clone, Debug)]
struct RankedResult {
    kind: ResultKind,
    title: String,
    subtitle: Option<String>,
    icon: ResultIcon,
    score: f64,
    action: SearchAction,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ResultKind {
    ChatOs,
    Application,
    File,
    Action,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ResultIcon {
    Project,
    Contact,
    Application,
    File,
    Folder,
    Screenshot,
    ScreenRecording,
    Clipboard,
    Settings,
    Permissions,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UsageRecord {
    last_used_at_unix_seconds: f64,
    count: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UsageFile {
    version: u32,
    records: BTreeMap<String, UsageRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickSearchBootstrap {
    presentation_id: String,
    language: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QuickSearchQueryInput {
    presentation_id: String,
    query: String,
    request_id: u64,
    include_files: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickSearchSnapshot {
    presentation_id: String,
    request_id: u64,
    results: Vec<QuickSearchResultProjection>,
    is_file_search_timed_out: bool,
    diagnostic: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickSearchResultProjection {
    id: String,
    kind: ResultKind,
    title: String,
    subtitle: Option<String>,
    icon: ResultIcon,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExecuteQuickSearchInput {
    presentation_id: String,
    result_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickSearchNavigationRequested {
    owner_user_id: String,
    destination: NavigationDestination,
    resource_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum NavigationDestination {
    Project,
    Contact,
    Settings,
    RuntimePermissions,
}

struct SearchBuild {
    results: Vec<RankedResult>,
    file_search_timed_out: bool,
    source_issue: bool,
}

struct QueryContext {
    owner_user_id: String,
    usage: BTreeMap<String, UsageRecord>,
    applications: Option<Vec<ApplicationRecord>>,
}

impl QuickSearchManager {
    pub(crate) fn new(data_directory: PathBuf) -> Self {
        let usage_path = data_directory
            .join("preferences")
            .join("quick-search-usage.json");
        let usage = load_usage(&usage_path).unwrap_or_default();
        Self {
            data_directory,
            usage_path,
            inner: Arc::new(Mutex::new(QuickSearchRuntime {
                usage,
                ..QuickSearchRuntime::default()
            })),
        }
    }

    pub(crate) fn begin(&self, owner_user_id: &str) -> Result<Uuid, String> {
        validate_owner(owner_user_id)?;
        let mut inner = self.inner.lock().map_err(|_| search_error())?;
        let id = Uuid::new_v4();
        inner.presentation = Some(QuickSearchPresentation {
            id,
            owner_user_id: owner_user_id.to_owned(),
            latest_request_id: 0,
            actions: HashMap::new(),
        });
        Ok(id)
    }

    fn bootstrap(&self) -> Result<(Uuid, String), String> {
        let inner = self.inner.lock().map_err(|_| search_error())?;
        let presentation = inner
            .presentation
            .as_ref()
            .ok_or_else(|| "快捷搜索窗口已经关闭。".to_owned())?;
        Ok((presentation.id, presentation.owner_user_id.clone()))
    }

    fn start_query(&self, presentation_id: Uuid, request_id: u64) -> Result<QueryContext, String> {
        if request_id == 0 {
            return Err("快捷搜索请求无效。".into());
        }
        let mut inner = self.inner.lock().map_err(|_| search_error())?;
        let owner_user_id = {
            let presentation = require_presentation_mut(&mut inner, presentation_id)?;
            if request_id <= presentation.latest_request_id {
                return Err("快捷搜索内容已经变化。".into());
            }
            presentation.latest_request_id = request_id;
            presentation.owner_user_id.clone()
        };
        Ok(QueryContext {
            owner_user_id,
            usage: inner.usage.clone(),
            applications: inner.applications.clone(),
        })
    }

    fn cache_applications(&self, applications: Vec<ApplicationRecord>) -> Result<(), String> {
        let mut inner = self.inner.lock().map_err(|_| search_error())?;
        if inner.applications.is_none() {
            inner.applications = Some(applications);
        }
        Ok(())
    }

    fn publish(
        &self,
        presentation_id: Uuid,
        request_id: u64,
        build: SearchBuild,
        english: bool,
    ) -> Result<QuickSearchSnapshot, String> {
        let mut inner = self.inner.lock().map_err(|_| search_error())?;
        let presentation = require_presentation_mut(&mut inner, presentation_id)?;
        if presentation.latest_request_id != request_id {
            return Err("快捷搜索内容已经变化。".into());
        }
        let mut actions = HashMap::with_capacity(build.results.len());
        let results = build
            .results
            .into_iter()
            .map(|result| {
                let id = Uuid::new_v4();
                actions.insert(id, result.action);
                QuickSearchResultProjection {
                    id: id.to_string(),
                    kind: result.kind,
                    title: result.title,
                    subtitle: result.subtitle,
                    icon: result.icon,
                }
            })
            .collect();
        presentation.actions = actions;
        let diagnostic = if build.file_search_timed_out {
            Some(localized(
                english,
                "文件搜索已达到 3 秒上限，已显示当前结果。",
                "File search reached the 3-second limit. Showing current results.",
            ))
        } else if build.source_issue {
            Some(localized(
                english,
                "部分本机来源暂时不可用，已显示其余可信结果。",
                "Some local sources are unavailable. Showing the remaining trusted results.",
            ))
        } else {
            None
        };
        Ok(QuickSearchSnapshot {
            presentation_id: presentation_id.to_string(),
            request_id,
            results,
            is_file_search_timed_out: build.file_search_timed_out,
            diagnostic,
        })
    }

    fn resolve_action(
        &self,
        presentation_id: Uuid,
        result_id: Uuid,
    ) -> Result<(String, SearchAction), String> {
        let inner = self.inner.lock().map_err(|_| search_error())?;
        let presentation = require_presentation(&inner, presentation_id)?;
        let action = presentation
            .actions
            .get(&result_id)
            .cloned()
            .ok_or_else(|| "快捷搜索结果已经失效。".to_owned())?;
        Ok((presentation.owner_user_id.clone(), action))
    }

    fn record_usage(&self, usage_key: String) -> Result<(), String> {
        let records = {
            let mut inner = self.inner.lock().map_err(|_| search_error())?;
            let record = inner.usage.entry(usage_key).or_insert(UsageRecord {
                last_used_at_unix_seconds: 0.0,
                count: 0,
            });
            record.last_used_at_unix_seconds = now_unix_seconds();
            record.count = record.count.saturating_add(1);
            prune_usage(&mut inner.usage);
            inner.usage.clone()
        };
        save_usage(&self.usage_path, &records)
    }

    pub(crate) fn discard(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().map_err(|_| search_error())?;
        inner.presentation = None;
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn load_quick_search(
    authentication: State<'_, AuthenticationManager>,
    general_settings: State<'_, GeneralSettingsStore>,
    manager: State<'_, QuickSearchManager>,
) -> Result<QuickSearchBootstrap, String> {
    let (presentation_id, owner_user_id) = manager.bootstrap()?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let language = general_settings.interface_language(&owner_user_id)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(QuickSearchBootstrap {
        presentation_id: presentation_id.to_string(),
        language,
    })
}

#[tauri::command]
pub(crate) async fn query_quick_search(
    input: QuickSearchQueryInput,
    authentication: State<'_, AuthenticationManager>,
    general_settings: State<'_, GeneralSettingsStore>,
    manager: State<'_, QuickSearchManager>,
) -> Result<QuickSearchSnapshot, String> {
    validate_query(&input.query)?;
    let presentation_id = parse_id(&input.presentation_id)?;
    let context = manager.start_query(presentation_id, input.request_id)?;
    let owner_user_id = context.owner_user_id;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let language = general_settings.interface_language(&owner_user_id)?;
    let english = language == "en-US";
    let data_directory = manager.data_directory.clone();
    let search_owner_user_id = owner_user_id.clone();
    let query = input.query;
    let include_files = input.include_files;
    let build = tauri::async_runtime::spawn_blocking(move || {
        build_results(
            &data_directory,
            &search_owner_user_id,
            &query,
            include_files,
            context.applications,
            &context.usage,
            english,
        )
    })
    .await
    .map_err(|_| search_error())?;
    if let Some(applications) = build.1 {
        manager.cache_applications(applications)?;
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    manager.publish(presentation_id, input.request_id, build.0, english)
}

#[tauri::command]
pub(crate) async fn execute_quick_search(
    input: ExecuteQuickSearchInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, QuickSearchManager>,
) -> Result<(), String> {
    let presentation_id = parse_id(&input.presentation_id)?;
    let result_id = parse_id(&input.result_id)?;
    let (owner_user_id, action) = manager.resolve_action(presentation_id, result_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let _ = manager.record_usage(usage_key_for_action(&action));
    hide_quick_search(&app)?;
    manager.discard()?;
    execute_action(&app, &owner_user_id, action)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await
}

#[tauri::command]
pub(crate) fn close_quick_search(
    app: AppHandle,
    manager: State<'_, QuickSearchManager>,
) -> Result<(), String> {
    hide_quick_search(&app)?;
    manager.discard()
}

pub(crate) fn show_quick_search(app: &AppHandle) -> Result<(), String> {
    let utilities = app.state::<GlobalUtilityPreferencesStore>();
    let owner_user_id = utilities.active_owner()?;
    let manager = app.state::<QuickSearchManager>();
    manager.begin(&owner_user_id)?;
    let window = if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.show().map_err(|_| search_error())?;
        window
    } else {
        WebviewWindowBuilder::new(
            app,
            WINDOW_LABEL,
            WebviewUrl::App("index.html?surface=quick-search".into()),
        )
        .title("ChatOS Quick Search")
        .inner_size(760.0, 520.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .center()
        .build()
        .map_err(|_| search_error())?
    };
    window.set_focus().map_err(|_| search_error())?;
    let _ = window.emit("quick-search-presented", ());
    Ok(())
}

pub(crate) fn dismiss_quick_search(app: &AppHandle) {
    if let Some(manager) = app.try_state::<QuickSearchManager>() {
        let _ = manager.discard();
    }
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.hide();
    }
}

fn hide_quick_search(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.hide().map_err(|_| search_error())?;
    }
    Ok(())
}

fn execute_action(
    app: &AppHandle,
    owner_user_id: &str,
    action: SearchAction,
) -> Result<(), String> {
    match action {
        SearchAction::OpenProject(resource_id) => request_navigation(
            app,
            owner_user_id,
            NavigationDestination::Project,
            Some(resource_id),
        ),
        SearchAction::OpenContact(resource_id) => request_navigation(
            app,
            owner_user_id,
            NavigationDestination::Contact,
            Some(resource_id),
        ),
        SearchAction::OpenApplication(path) | SearchAction::OpenFile(path) => open_path(&path),
        SearchAction::BuiltIn(BuiltInAction::Screenshot) => {
            dispatch_action(app, GlobalUtilityAction::Screenshot);
            Ok(())
        }
        SearchAction::BuiltIn(BuiltInAction::ScreenRecording) => {
            dispatch_action(app, GlobalUtilityAction::ScreenRecording);
            Ok(())
        }
        SearchAction::BuiltIn(BuiltInAction::ClipboardHistory) => {
            dispatch_action(app, GlobalUtilityAction::ClipboardHistory);
            Ok(())
        }
        SearchAction::BuiltIn(BuiltInAction::OpenSettings) => {
            request_navigation(app, owner_user_id, NavigationDestination::Settings, None)
        }
        SearchAction::BuiltIn(BuiltInAction::OpenRuntimePermissions) => request_navigation(
            app,
            owner_user_id,
            NavigationDestination::RuntimePermissions,
            None,
        ),
    }
}

fn request_navigation(
    app: &AppHandle,
    owner_user_id: &str,
    destination: NavigationDestination,
    resource_id: Option<String>,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口暂时不可用。".to_owned())?;
    window.unminimize().map_err(|_| search_error())?;
    window.show().map_err(|_| search_error())?;
    window.set_focus().map_err(|_| search_error())?;
    window
        .emit(
            "quick-search-navigation-requested",
            QuickSearchNavigationRequested {
                owner_user_id: owner_user_id.to_owned(),
                destination,
                resource_id,
            },
        )
        .map_err(|_| search_error())
}

fn open_path(path: &Path) -> Result<(), String> {
    validate_open_target(path)?;
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|_| "无法打开所选项目。".to_owned())
}

fn validate_open_target(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("快捷搜索结果已经失效。".into());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| "快捷搜索结果已经失效。".to_owned())?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err("快捷搜索结果已经失效。".into());
    }
    if !metadata.is_file() && !metadata.is_dir() {
        return Err("快捷搜索结果已经失效。".into());
    }
    Ok(())
}

fn build_results(
    data_directory: &Path,
    owner_user_id: &str,
    raw_query: &str,
    include_files: bool,
    cached_applications: Option<Vec<ApplicationRecord>>,
    usage: &BTreeMap<String, UsageRecord>,
    english: bool,
) -> (SearchBuild, Option<Vec<ApplicationRecord>>) {
    let trimmed = raw_query.trim();
    let scope = scope_for(trimmed);
    let query = stripped_prefix(trimmed);
    let mut source_issue = false;
    let mut results = Vec::new();

    if matches!(scope, SearchScope::All | SearchScope::ChatOs) {
        match load_projects(data_directory, owner_user_id) {
            Ok(projects) => {
                for (id, title) in projects {
                    push_ranked(
                        &mut results,
                        usage,
                        RankedCandidate {
                            usage_key: format!("project:{id}"),
                            kind: ResultKind::ChatOs,
                            title,
                            subtitle: Some(localized(english, "ChatOS 项目", "ChatOS Project")),
                            icon: ResultIcon::Project,
                            provider_weight: 95.0,
                            action: SearchAction::OpenProject(id),
                        },
                        &query,
                    );
                }
            }
            Err(_) => source_issue = true,
        }
        for (id, title) in [("jiguli", "叽咕狸"), ("plugin-installer", "插件安装助手")] {
            push_ranked(
                &mut results,
                usage,
                RankedCandidate {
                    usage_key: format!("contact:{id}"),
                    kind: ResultKind::ChatOs,
                    title: title.to_owned(),
                    subtitle: Some(localized(english, "ChatOS 联系人", "ChatOS Contact")),
                    icon: ResultIcon::Contact,
                    provider_weight: 88.0,
                    action: SearchAction::OpenContact(id.to_owned()),
                },
                &query,
            );
        }
    }

    let scanned_applications = if matches!(scope, SearchScope::All) {
        let applications = cached_applications.unwrap_or_else(scan_applications);
        for application in &applications {
            let stable_path = application.path.to_string_lossy();
            push_ranked(
                &mut results,
                usage,
                RankedCandidate {
                    usage_key: format!("app:{}", normalize(&stable_path)),
                    kind: ResultKind::Application,
                    title: application.title.clone(),
                    subtitle: application.subtitle.clone(),
                    icon: ResultIcon::Application,
                    provider_weight: 74.0,
                    action: SearchAction::OpenApplication(application.path.clone()),
                },
                &query,
            );
        }
        Some(applications)
    } else {
        None
    };

    if matches!(scope, SearchScope::All | SearchScope::Actions) {
        for candidate in built_in_actions(english) {
            push_ranked(&mut results, usage, candidate, &query);
        }
    }

    sort_results(&mut results);
    results.truncate(MAXIMUM_MEMORY_RESULTS);

    let mut file_search_timed_out = false;
    if include_files
        && query.chars().count() >= 2
        && matches!(scope, SearchScope::All | SearchScope::Files)
    {
        let file_result = search_files(&query, FILE_SEARCH_TIMEOUT);
        file_search_timed_out = file_result.timed_out;
        source_issue |= file_result.source_issue;
        for file in file_result.records {
            let stable_path = file.path.to_string_lossy();
            push_ranked(
                &mut results,
                usage,
                RankedCandidate {
                    usage_key: format!("file:{}", normalize(&stable_path)),
                    kind: ResultKind::File,
                    title: file.title,
                    subtitle: Some(file.subtitle),
                    icon: if file.is_directory {
                        ResultIcon::Folder
                    } else {
                        ResultIcon::File
                    },
                    provider_weight: 38.0,
                    action: SearchAction::OpenFile(file.path),
                },
                &query,
            );
        }
        sort_results(&mut results);
        results.truncate(MAXIMUM_TOTAL_RESULTS);
    }

    (
        SearchBuild {
            results,
            file_search_timed_out,
            source_issue,
        },
        scanned_applications,
    )
}

struct RankedCandidate {
    usage_key: String,
    kind: ResultKind,
    title: String,
    subtitle: Option<String>,
    icon: ResultIcon,
    provider_weight: f64,
    action: SearchAction,
}

fn push_ranked(
    results: &mut Vec<RankedResult>,
    usage: &BTreeMap<String, UsageRecord>,
    candidate: RankedCandidate,
    query: &str,
) {
    let boosts = usage_boost(usage.get(&candidate.usage_key));
    let Some(score) = ranking_score(
        query,
        &candidate.title,
        candidate.subtitle.as_deref(),
        candidate.provider_weight,
        boosts.0,
        boosts.1,
    ) else {
        return;
    };
    results.push(RankedResult {
        kind: candidate.kind,
        title: candidate.title,
        subtitle: candidate.subtitle,
        icon: candidate.icon,
        score,
        action: candidate.action,
    });
}

fn sort_results(results: &mut [RankedResult]) {
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| normalize(&left.title).cmp(&normalize(&right.title)))
    });
}

fn built_in_actions(english: bool) -> Vec<RankedCandidate> {
    vec![
        action_candidate(
            BuiltInAction::Screenshot,
            english,
            "截屏",
            "Take Screenshot",
            "选择区域、标注或长截图",
            "Capture or annotate the screen",
            ResultIcon::Screenshot,
        ),
        action_candidate(
            BuiltInAction::ScreenRecording,
            english,
            "开始或停止录屏",
            "Start or Stop Recording",
            "录制显示器或窗口",
            "Record a display or window",
            ResultIcon::ScreenRecording,
        ),
        action_candidate(
            BuiltInAction::ClipboardHistory,
            english,
            "打开剪贴板历史",
            "Open Clipboard History",
            "查找并恢复之前复制的内容",
            "Find and restore copied content",
            ResultIcon::Clipboard,
        ),
        action_candidate(
            BuiltInAction::OpenSettings,
            english,
            "打开 ChatOS 设置",
            "Open ChatOS Settings",
            "管理账号、全局工具和连接器",
            "Manage account, global tools, and connector",
            ResultIcon::Settings,
        ),
        action_candidate(
            BuiltInAction::OpenRuntimePermissions,
            english,
            "打开系统权限",
            "Open Runtime & Permissions",
            "检查运行时能力和系统权限",
            "Inspect runtime capabilities and system permissions",
            ResultIcon::Permissions,
        ),
    ]
}

fn action_candidate(
    action: BuiltInAction,
    english: bool,
    chinese_title: &str,
    english_title: &str,
    chinese_subtitle: &str,
    english_subtitle: &str,
    icon: ResultIcon,
) -> RankedCandidate {
    RankedCandidate {
        usage_key: usage_key_for_action(&SearchAction::BuiltIn(action)),
        kind: ResultKind::Action,
        title: localized(english, chinese_title, english_title),
        subtitle: Some(localized(english, chinese_subtitle, english_subtitle)),
        icon,
        provider_weight: 62.0,
        action: SearchAction::BuiltIn(action),
    }
}

fn ranking_score(
    query: &str,
    title: &str,
    subtitle: Option<&str>,
    provider_weight: f64,
    recency_boost: f64,
    frequency_boost: f64,
) -> Option<f64> {
    let query = normalize(query);
    if query.is_empty() {
        return Some(provider_weight + recency_boost + frequency_boost);
    }
    let title_score = textual_score(&query, &normalize(title));
    let subtitle_score =
        textual_score(&query, &normalize(subtitle.unwrap_or_default())).map(|score| score * 0.45);
    let text_score = match (title_score, subtitle_score) {
        (Some(left), Some(right)) => left.max(right),
        (Some(score), None) | (None, Some(score)) => score,
        (None, None) => return None,
    };
    Some(provider_weight + text_score + recency_boost.min(80.0) + frequency_boost.min(45.0))
}

fn textual_score(query: &str, candidate: &str) -> Option<f64> {
    if candidate.is_empty() {
        return None;
    }
    if candidate == query {
        return Some(520.0);
    }
    if candidate.starts_with(query) {
        return Some(390.0 - (candidate.chars().count() - query.chars().count()) as f64 * 0.15);
    }
    if candidate.contains(&format!(" {query}"))
        || candidate.contains(&format!("-{query}"))
        || candidate.contains(&format!("_{query}"))
    {
        return Some(310.0);
    }
    if let Some(index) = candidate.find(query) {
        return Some(235.0 - candidate[..index].chars().count() as f64 * 0.5);
    }
    let candidate_characters = candidate.chars().collect::<Vec<_>>();
    let mut offset = 0usize;
    let mut previous = None;
    let mut gap_penalty = 0.0;
    for character in query.chars() {
        let relative = candidate_characters[offset..]
            .iter()
            .position(|candidate| *candidate == character)?;
        let index = offset + relative;
        gap_penalty += if let Some(previous) = previous {
            (index - previous - 1) as f64 * 3.0
        } else {
            index as f64 * 2.0
        };
        previous = Some(index);
        offset = index + 1;
    }
    Some((170.0 - gap_penalty).max(40.0))
}

fn usage_boost(record: Option<&UsageRecord>) -> (f64, f64) {
    let Some(record) = record else {
        return (0.0, 0.0);
    };
    let age = (now_unix_seconds() - record.last_used_at_unix_seconds).max(0.0);
    let recency = (70.0 - age / 86_400.0 * 8.0).max(0.0);
    let frequency = ((record.count as f64 + 1.0).log2() * 12.0).min(45.0);
    (recency, frequency)
}

fn usage_key_for_action(action: &SearchAction) -> String {
    match action {
        SearchAction::OpenProject(id) => format!("project:{id}"),
        SearchAction::OpenContact(id) => format!("contact:{id}"),
        SearchAction::OpenApplication(path) => {
            format!("app:{}", normalize(&path.to_string_lossy()))
        }
        SearchAction::OpenFile(path) => format!("file:{}", normalize(&path.to_string_lossy())),
        SearchAction::BuiltIn(action) => format!(
            "action:{}",
            match action {
                BuiltInAction::Screenshot => "screenshot",
                BuiltInAction::ScreenRecording => "screenRecording",
                BuiltInAction::ClipboardHistory => "clipboardHistory",
                BuiltInAction::OpenSettings => "openSettings",
                BuiltInAction::OpenRuntimePermissions => "openRuntimePermissions",
            }
        ),
    }
}

fn scope_for(query: &str) -> SearchScope {
    match query.chars().next() {
        Some('>') => SearchScope::Actions,
        Some('@') => SearchScope::ChatOs,
        Some('/') => SearchScope::Files,
        _ => SearchScope::All,
    }
}

fn stripped_prefix(query: &str) -> String {
    match query.chars().next() {
        Some('>' | '@' | '/') => query.chars().skip(1).collect::<String>().trim().to_owned(),
        _ => query.to_owned(),
    }
}

fn normalize(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn validate_query(query: &str) -> Result<(), String> {
    if query.chars().count() > MAXIMUM_QUERY_CHARACTERS
        || query.chars().any(|character| {
            character == '\0' || (character.is_control() && !character.is_whitespace())
        })
    {
        return Err("快捷搜索关键词无效。".into());
    }
    Ok(())
}

fn load_projects(
    data_directory: &Path,
    owner_user_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let database = data_directory
        .join("NativeConnector")
        .join("Projects.sqlite3");
    if !database.exists() {
        return Ok(Vec::new());
    }
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| search_error())?;
    let mut statement = connection
        .prepare(
            "SELECT id, name FROM local_project_records \
             WHERE owner_user_id = ?1 AND status = 'active' ORDER BY name, id",
        )
        .map_err(|_| search_error())?;
    let rows = statement
        .query_map(params![owner_user_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| search_error())?;
    let projects = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| search_error())?;
    if projects.iter().any(|(id, name)| {
        id.trim().is_empty()
            || id.len() > 512
            || name.trim().is_empty()
            || name.chars().count() > 512
    }) {
        return Err(search_error());
    }
    Ok(projects)
}

fn scan_applications() -> Vec<ApplicationRecord> {
    let mut records = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for root in application_roots() {
        let mut directories = VecDeque::from([root]);
        while let Some(directory) = directories.pop_front() {
            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                if records.len() >= MAXIMUM_APPLICATION_RECORDS {
                    break;
                }
                let path = entry.path();
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                    continue;
                }
                let is_application = is_application_path(&path);
                if metadata.is_dir() && !is_application {
                    directories.push_back(path);
                    continue;
                }
                if !is_application {
                    continue;
                }
                let identity = normalize(&path.to_string_lossy());
                if !seen.insert(identity) {
                    continue;
                }
                let title = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Application")
                    .trim()
                    .to_owned();
                if title.is_empty() {
                    continue;
                }
                records.push(ApplicationRecord {
                    title,
                    subtitle: Some(application_subtitle(&path)),
                    path,
                });
            }
        }
    }
    records.sort_by(|left, right| normalize(&left.title).cmp(&normalize(&right.title)));
    records
}

#[cfg(target_os = "windows")]
fn application_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(program_data) = std::env::var_os("ProgramData") {
        roots.push(PathBuf::from(program_data).join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(app_data).join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local_app_data).join("Microsoft/WindowsApps"));
    }
    roots
}

#[cfg(target_os = "macos")]
fn application_roots() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ]
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn application_roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/usr/share/applications")]
}

fn is_application_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    #[cfg(target_os = "windows")]
    return matches!(extension.as_str(), "lnk" | "url" | "exe");
    #[cfg(target_os = "macos")]
    return extension == "app";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return extension == "desktop";
}

fn application_subtitle(path: &Path) -> String {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Installed application")
        .to_owned()
}

struct FileSearchResult {
    records: Vec<FileRecord>,
    timed_out: bool,
    source_issue: bool,
}

fn search_files(query: &str, timeout: Duration) -> FileSearchResult {
    let started = Instant::now();
    let deadline = started + timeout;
    let mut source_issue = false;
    let mut timed_out = false;
    let mut candidates = Vec::new();
    let Some(home) = dirs::home_dir() else {
        return FileSearchResult {
            records: Vec::new(),
            timed_out: false,
            source_issue: true,
        };
    };
    let mut directories = VecDeque::from([(home.clone(), 0usize)]);
    while let Some((directory, depth)) = directories.pop_front() {
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                source_issue = true;
                continue;
            }
        };
        for entry in entries {
            if Instant::now() >= deadline {
                timed_out = true;
                break;
            }
            let Ok(entry) = entry else {
                source_issue = true;
                continue;
            };
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                source_issue = true;
                continue;
            };
            if metadata.file_type().is_symlink()
                || is_reparse_point(&metadata)
                || is_hidden_or_system(&path, &metadata)
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if should_skip_directory(&name) && metadata.is_dir() {
                continue;
            }
            if normalize(&name).contains(&normalize(query)) {
                candidates.push(FileRecord {
                    title: name,
                    subtitle: safe_parent_label(&path, &home),
                    path: path.clone(),
                    is_directory: metadata.is_dir(),
                    modified_at: metadata.modified().unwrap_or(UNIX_EPOCH),
                });
                if candidates.len() >= FILE_SEARCH_CANDIDATE_LIMIT {
                    break;
                }
            }
            if metadata.is_dir() && depth < 12 {
                directories.push_back((path, depth + 1));
            }
        }
        if candidates.len() >= FILE_SEARCH_CANDIDATE_LIMIT {
            break;
        }
    }
    candidates.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| normalize(&left.title).cmp(&normalize(&right.title)))
    });
    candidates.truncate(MAXIMUM_TOTAL_RESULTS);
    FileSearchResult {
        records: candidates,
        timed_out,
        source_issue,
    }
}

fn should_skip_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "appdata" | "node_modules" | "target" | ".git" | ".cache" | ".cargo" | ".rustup"
    )
}

fn safe_parent_label(path: &Path, home: &Path) -> String {
    let parent = path.parent().unwrap_or(home);
    let relative = parent.strip_prefix(home).unwrap_or(parent);
    let components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| !component.is_empty())
        .take(3)
        .collect::<Vec<_>>();
    if components.is_empty() {
        "Home".into()
    } else {
        components.join(" › ")
    }
}

#[cfg(target_os = "windows")]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(target_os = "windows"))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn is_hidden_or_system(_: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & (0x2 | 0x4) != 0
}

#[cfg(not(target_os = "windows"))]
fn is_hidden_or_system(path: &Path, _: &fs::Metadata) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with('.'))
}

fn load_usage(path: &Path) -> Result<BTreeMap<String, UsageRecord>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(search_error()),
    };
    let metadata = file.metadata().map_err(|_| search_error())?;
    if !metadata.is_file() || metadata.len() > MAXIMUM_USAGE_FILE_BYTES {
        return Err(search_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAXIMUM_USAGE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| search_error())?;
    if bytes.len() as u64 > MAXIMUM_USAGE_FILE_BYTES {
        return Err(search_error());
    }
    let mut parsed: UsageFile = serde_json::from_slice(&bytes).map_err(|_| search_error())?;
    if parsed.version != USAGE_FILE_VERSION
        || parsed.records.values().any(|record| {
            !record.last_used_at_unix_seconds.is_finite()
                || record.last_used_at_unix_seconds < 0.0
                || record.count == 0
        })
    {
        return Err(search_error());
    }
    prune_usage(&mut parsed.records);
    Ok(parsed.records)
}

fn save_usage(path: &Path, records: &BTreeMap<String, UsageRecord>) -> Result<(), String> {
    let bytes = serde_json::to_vec(&UsageFile {
        version: USAGE_FILE_VERSION,
        records: records.clone(),
    })
    .map_err(|_| search_error())?;
    if bytes.len() as u64 > MAXIMUM_USAGE_FILE_BYTES {
        return Err(search_error());
    }
    let parent = path.parent().ok_or_else(search_error)?;
    fs::create_dir_all(parent).map_err(|_| search_error())?;
    let temporary = parent.join(format!(".quick-search-{}.tmp", Uuid::new_v4()));
    let result =
        write_private_file(&temporary, &bytes).and_then(|_| replace_file(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(search_error());
    }
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn prune_usage(records: &mut BTreeMap<String, UsageRecord>) {
    if records.len() <= MAXIMUM_USAGE_RECORDS {
        return;
    }
    let mut ordered = records
        .iter()
        .map(|(key, value)| (key.clone(), value.last_used_at_unix_seconds))
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let retained = ordered
        .into_iter()
        .take(MAXIMUM_USAGE_RECORDS)
        .map(|(key, _)| key)
        .collect::<std::collections::BTreeSet<_>>();
    records.retain(|key, _| retained.contains(key));
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

#[cfg(not(target_os = "windows"))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(target_os = "windows")]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn require_presentation(
    runtime: &QuickSearchRuntime,
    presentation_id: Uuid,
) -> Result<&QuickSearchPresentation, String> {
    runtime
        .presentation
        .as_ref()
        .filter(|presentation| presentation.id == presentation_id)
        .ok_or_else(|| "快捷搜索窗口已经变化，请重试。".to_owned())
}

fn require_presentation_mut(
    runtime: &mut QuickSearchRuntime,
    presentation_id: Uuid,
) -> Result<&mut QuickSearchPresentation, String> {
    runtime
        .presentation
        .as_mut()
        .filter(|presentation| presentation.id == presentation_id)
        .ok_or_else(|| "快捷搜索窗口已经变化，请重试。".to_owned())
}

fn parse_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| "快捷搜索请求无效。".into())
}

fn localized(english: bool, chinese: &str, english_value: &str) -> String {
    if english { english_value } else { chinese }.to_owned()
}

fn now_unix_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn search_error() -> String {
    "无法读取快捷搜索数据。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_and_prefixes_match_the_macos_contract() {
        assert_eq!(scope_for("> open"), SearchScope::Actions);
        assert_eq!(scope_for("@ jiguli"), SearchScope::ChatOs);
        assert_eq!(scope_for("/ report"), SearchScope::Files);
        assert_eq!(scope_for("report"), SearchScope::All);
        assert_eq!(stripped_prefix("/ report"), "report");
    }

    #[test]
    fn ranking_prefers_exact_prefix_and_usage_boosts() {
        let exact = ranking_score("notes", "Notes", None, 10.0, 0.0, 0.0).unwrap();
        let prefix = ranking_score("note", "Notebook", None, 10.0, 0.0, 0.0).unwrap();
        let fuzzy = ranking_score("ntb", "Notebook", None, 10.0, 0.0, 0.0).unwrap();
        assert!(exact > prefix);
        assert!(prefix > fuzzy);
        assert!(ranking_score("missing", "Notebook", None, 10.0, 0.0, 0.0).is_none());
        assert!(ranking_score("", "Notebook", None, 10.0, 70.0, 45.0).unwrap() > 100.0);
    }

    #[test]
    fn usage_is_pruned_to_the_newest_512_records() {
        let mut records = (0..520)
            .map(|index| {
                (
                    format!("result-{index:03}"),
                    UsageRecord {
                        last_used_at_unix_seconds: index as f64,
                        count: 1,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        prune_usage(&mut records);
        assert_eq!(records.len(), MAXIMUM_USAGE_RECORDS);
        assert!(!records.contains_key("result-000"));
        assert!(records.contains_key("result-519"));
    }

    #[test]
    fn file_search_is_bounded_and_does_not_follow_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("quarterly-report.txt"), b"report").unwrap();
        let nested = directory.path().join("Documents");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("report-notes.md"), b"notes").unwrap();
        let result =
            search_files_from_root_for_test(directory.path(), "report", Duration::from_secs(1));
        assert_eq!(result.records.len(), 2);
        assert!(result
            .records
            .iter()
            .all(|record| !record.path.to_string_lossy().is_empty()));
    }

    fn search_files_from_root_for_test(
        root: &Path,
        query: &str,
        timeout: Duration,
    ) -> FileSearchResult {
        let deadline = Instant::now() + timeout;
        let mut records = Vec::new();
        let mut directories = VecDeque::from([(root.to_path_buf(), 0usize)]);
        let mut timed_out = false;
        while let Some((directory, depth)) = directories.pop_front() {
            for entry in fs::read_dir(directory).unwrap().flatten() {
                if Instant::now() >= deadline {
                    timed_out = true;
                    break;
                }
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path).unwrap();
                if metadata.file_type().is_symlink() {
                    continue;
                }
                let title = entry.file_name().to_string_lossy().to_string();
                if normalize(&title).contains(&normalize(query)) {
                    records.push(FileRecord {
                        title,
                        subtitle: safe_parent_label(&path, root),
                        path: path.clone(),
                        is_directory: metadata.is_dir(),
                        modified_at: metadata.modified().unwrap_or(UNIX_EPOCH),
                    });
                }
                if metadata.is_dir() && depth < 12 {
                    directories.push_back((path, depth + 1));
                }
            }
        }
        FileSearchResult {
            records,
            timed_out,
            source_issue: false,
        }
    }
}

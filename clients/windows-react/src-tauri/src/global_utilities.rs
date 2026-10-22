use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};
use uuid::Uuid;

use crate::{authentication::AuthenticationManager, host_lifecycle::validate_owner};

const FILE_VERSION: u32 = 1;
const MAXIMUM_FILE_BYTES: u64 = 64 * 1_024;
const TRIGGER_DEBOUNCE: Duration = Duration::from_millis(350);

#[derive(Clone)]
pub(crate) struct GlobalUtilityPreferencesStore {
    path: PathBuf,
    file_lock: Arc<Mutex<()>>,
    registration_lock: Arc<Mutex<()>>,
    runtime: Arc<Mutex<GlobalUtilityRuntime>>,
}

#[derive(Default)]
struct GlobalUtilityRuntime {
    active_owner: Option<String>,
    registered: Vec<Shortcut>,
    registrations: BTreeMap<GlobalUtilityAction, GlobalShortcutRegistration>,
    last_triggers: BTreeMap<GlobalUtilityAction, Instant>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub(crate) enum GlobalUtilityAction {
    Screenshot,
    ScreenRecording,
    ClipboardHistory,
    QuickSearch,
}

impl GlobalUtilityAction {
    const ALL: [Self; 4] = [
        Self::Screenshot,
        Self::ScreenRecording,
        Self::ClipboardHistory,
        Self::QuickSearch,
    ];

    fn fallback(self) -> Option<GlobalShortcutSpec> {
        (self == Self::QuickSearch).then(|| GlobalShortcutSpec::new("Space", &["control", "shift"]))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GlobalShortcutSpec {
    key: String,
    modifiers: Vec<String>,
}

impl GlobalShortcutSpec {
    fn new(key: &str, modifiers: &[&str]) -> Self {
        Self {
            key: key.into(),
            modifiers: modifiers.iter().map(|value| (*value).into()).collect(),
        }
    }

    fn plugin_string(&self) -> Result<String, String> {
        validate_shortcut(self)?;
        let modifiers = self
            .modifiers
            .iter()
            .map(|modifier| match modifier.as_str() {
                "control" => "Ctrl",
                "alt" => "Alt",
                "shift" => "Shift",
                "super" => "Super",
                _ => unreachable!("validated shortcut modifier"),
            })
            .collect::<Vec<_>>()
            .join("+");
        Ok(format!("{modifiers}+{}", self.key))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GlobalUtilityPreferencesFile {
    version: u32,
    is_enabled: bool,
    has_acknowledged_shortcut_conflicts: bool,
    screenshot_enabled: bool,
    screen_recording_enabled: bool,
    clipboard_history_enabled: bool,
    quick_search_enabled: bool,
    hot_keys: BTreeMap<GlobalUtilityAction, GlobalShortcutSpec>,
    revision: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GlobalUtilityPreferencesSnapshot {
    is_enabled: bool,
    has_acknowledged_shortcut_conflicts: bool,
    screenshot_enabled: bool,
    screen_recording_enabled: bool,
    clipboard_history_enabled: bool,
    quick_search_enabled: bool,
    hot_keys: BTreeMap<GlobalUtilityAction, GlobalShortcutSpec>,
    registrations: BTreeMap<GlobalUtilityAction, GlobalShortcutRegistration>,
    revision: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GlobalShortcutRegistration {
    state: GlobalShortcutRegistrationState,
    requested: GlobalShortcutSpec,
    active: Option<GlobalShortcutSpec>,
    uses_fallback: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum GlobalShortcutRegistrationState {
    Registered,
    Conflict,
    Unsupported,
    Disabled,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateGlobalUtilityPreferencesInput {
    owner_user_id: String,
    is_enabled: bool,
    has_acknowledged_shortcut_conflicts: bool,
    screenshot_enabled: bool,
    screen_recording_enabled: bool,
    clipboard_history_enabled: bool,
    quick_search_enabled: bool,
    hot_keys: BTreeMap<GlobalUtilityAction, GlobalShortcutSpec>,
    expected_revision: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobalUtilityTriggered {
    action: GlobalUtilityAction,
}

impl Default for GlobalUtilityPreferencesFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            is_enabled: false,
            has_acknowledged_shortcut_conflicts: false,
            screenshot_enabled: true,
            screen_recording_enabled: true,
            clipboard_history_enabled: true,
            quick_search_enabled: true,
            hot_keys: default_hot_keys(),
            revision: 1,
        }
    }
}

impl GlobalUtilityPreferencesFile {
    fn action_enabled(&self, action: GlobalUtilityAction) -> bool {
        match action {
            GlobalUtilityAction::Screenshot => self.screenshot_enabled,
            GlobalUtilityAction::ScreenRecording => self.screen_recording_enabled,
            GlobalUtilityAction::ClipboardHistory => self.clipboard_history_enabled,
            GlobalUtilityAction::QuickSearch => self.quick_search_enabled,
        }
    }

    fn snapshot(
        &self,
        registrations: BTreeMap<GlobalUtilityAction, GlobalShortcutRegistration>,
    ) -> GlobalUtilityPreferencesSnapshot {
        GlobalUtilityPreferencesSnapshot {
            is_enabled: self.is_enabled,
            has_acknowledged_shortcut_conflicts: self.has_acknowledged_shortcut_conflicts,
            screenshot_enabled: self.screenshot_enabled,
            screen_recording_enabled: self.screen_recording_enabled,
            clipboard_history_enabled: self.clipboard_history_enabled,
            quick_search_enabled: self.quick_search_enabled,
            hot_keys: self.hot_keys.clone(),
            registrations,
            revision: self.revision,
        }
    }
}

impl GlobalUtilityPreferencesStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            path: app_data_directory
                .join("preferences")
                .join("global-utilities.json"),
            file_lock: Arc::new(Mutex::new(())),
            registration_lock: Arc::new(Mutex::new(())),
            runtime: Arc::new(Mutex::new(GlobalUtilityRuntime::default())),
        }
    }

    fn state(&self) -> Result<GlobalUtilityPreferencesFile, String> {
        let _guard = self.file_lock.lock().map_err(|_| utility_error())?;
        self.load()
    }

    fn update(
        &self,
        input: UpdateGlobalUtilityPreferencesInput,
    ) -> Result<GlobalUtilityPreferencesFile, String> {
        validate_update(&input)?;
        let _guard = self.file_lock.lock().map_err(|_| utility_error())?;
        let mut file = self.load()?;
        if file.revision != input.expected_revision {
            return Err("全局工具设置已经变化，请刷新后重试。".into());
        }
        file.is_enabled = input.is_enabled;
        file.has_acknowledged_shortcut_conflicts = input.has_acknowledged_shortcut_conflicts;
        file.screenshot_enabled = input.screenshot_enabled;
        file.screen_recording_enabled = input.screen_recording_enabled;
        file.clipboard_history_enabled = input.clipboard_history_enabled;
        file.quick_search_enabled = input.quick_search_enabled;
        file.hot_keys = input.hot_keys;
        file.revision = file.revision.checked_add(1).ok_or_else(utility_error)?;
        self.save(&file)?;
        Ok(file)
    }

    fn load(&self) -> Result<GlobalUtilityPreferencesFile, String> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(GlobalUtilityPreferencesFile::default())
            }
            Err(_) => return Err(utility_error()),
        };
        let metadata = file.metadata().map_err(|_| utility_error())?;
        if !metadata.is_file() || metadata.len() > MAXIMUM_FILE_BYTES {
            return Err(utility_error());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAXIMUM_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| utility_error())?;
        if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
            return Err(utility_error());
        }
        let parsed: GlobalUtilityPreferencesFile =
            serde_json::from_slice(&bytes).map_err(|_| utility_error())?;
        validate_file(&parsed)?;
        Ok(parsed)
    }

    fn save(&self, file: &GlobalUtilityPreferencesFile) -> Result<(), String> {
        validate_file(file)?;
        let bytes = serde_json::to_vec(file).map_err(|_| utility_error())?;
        if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
            return Err(utility_error());
        }
        let parent = self.path.parent().ok_or_else(utility_error)?;
        fs::create_dir_all(parent).map_err(|_| utility_error())?;
        let temporary = parent.join(format!(".global-utilities-{}.tmp", Uuid::new_v4()));
        let result =
            write_private_file(&temporary, &bytes).and_then(|_| fs::rename(&temporary, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(utility_error());
        }
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    }

    fn reconfigure(
        &self,
        app: &AppHandle,
        owner_user_id: &str,
        file: &GlobalUtilityPreferencesFile,
    ) -> Result<BTreeMap<GlobalUtilityAction, GlobalShortcutRegistration>, String> {
        let _registration_guard = self.registration_lock.lock().map_err(|_| utility_error())?;
        let previously_registered = {
            let mut runtime = self.runtime.lock().map_err(|_| utility_error())?;
            runtime.active_owner = Some(owner_user_id.into());
            runtime.registrations.clear();
            runtime.last_triggers.clear();
            std::mem::take(&mut runtime.registered)
        };
        for shortcut in previously_registered {
            let _ = app.global_shortcut().unregister(shortcut);
        }

        let mut registered = Vec::new();
        let mut registrations = BTreeMap::new();
        for action in GlobalUtilityAction::ALL {
            let requested = file
                .hot_keys
                .get(&action)
                .cloned()
                .ok_or_else(utility_error)?;
            if !file.is_enabled || !file.action_enabled(action) {
                registrations.insert(action, disabled_registration(requested));
                continue;
            }
            let (registration, shortcut) =
                self.register_action(app, action, requested.clone(), false);
            if let Some(shortcut) = shortcut {
                registered.push(shortcut);
                registrations.insert(action, registration);
                continue;
            }
            if let Some(fallback) = action.fallback().filter(|value| value != &requested) {
                let (fallback_registration, fallback_shortcut) =
                    self.register_action(app, action, fallback.clone(), true);
                if let Some(shortcut) = fallback_shortcut {
                    registered.push(shortcut);
                    registrations.insert(
                        action,
                        GlobalShortcutRegistration {
                            requested,
                            ..fallback_registration
                        },
                    );
                    continue;
                }
            }
            registrations.insert(action, registration);
        }

        let mut runtime = self.runtime.lock().map_err(|_| utility_error())?;
        if runtime.active_owner.as_deref() != Some(owner_user_id) {
            drop(runtime);
            for shortcut in registered {
                let _ = app.global_shortcut().unregister(shortcut);
            }
            return Err("登录账号已经变化，请重试。".into());
        }
        runtime.registered = registered;
        runtime.registrations = registrations.clone();
        Ok(registrations)
    }

    fn register_action(
        &self,
        app: &AppHandle,
        action: GlobalUtilityAction,
        spec: GlobalShortcutSpec,
        uses_fallback: bool,
    ) -> (GlobalShortcutRegistration, Option<Shortcut>) {
        let parsed = spec
            .plugin_string()
            .ok()
            .and_then(|value| Shortcut::from_str(&value).ok());
        let Some(shortcut) = parsed else {
            return (
                GlobalShortcutRegistration {
                    state: GlobalShortcutRegistrationState::Unsupported,
                    requested: spec,
                    active: None,
                    uses_fallback: false,
                },
                None,
            );
        };
        let trigger_store = self.clone();
        let result =
            app.global_shortcut()
                .on_shortcut(shortcut, move |app, _, event: ShortcutEvent| {
                    if event.state == ShortcutState::Pressed && trigger_store.should_trigger(action)
                    {
                        dispatch_action(app, action);
                    }
                });
        if result.is_ok() {
            (
                GlobalShortcutRegistration {
                    state: GlobalShortcutRegistrationState::Registered,
                    requested: spec.clone(),
                    active: Some(spec),
                    uses_fallback,
                },
                Some(shortcut),
            )
        } else {
            (
                GlobalShortcutRegistration {
                    state: GlobalShortcutRegistrationState::Conflict,
                    requested: spec,
                    active: None,
                    uses_fallback: false,
                },
                None,
            )
        }
    }

    fn should_trigger(&self, action: GlobalUtilityAction) -> bool {
        let Ok(mut runtime) = self.runtime.lock() else {
            return false;
        };
        let now = Instant::now();
        if runtime
            .last_triggers
            .get(&action)
            .is_some_and(|previous| now.duration_since(*previous) < TRIGGER_DEBOUNCE)
        {
            return false;
        }
        runtime.last_triggers.insert(action, now);
        true
    }

    fn deactivate(&self, app: &AppHandle, owner_user_id: &str) -> Result<(), String> {
        let _registration_guard = self.registration_lock.lock().map_err(|_| utility_error())?;
        let registered = {
            let mut runtime = self.runtime.lock().map_err(|_| utility_error())?;
            if runtime.active_owner.as_deref() != Some(owner_user_id) {
                return Ok(());
            }
            runtime.active_owner = None;
            runtime.registrations.clear();
            runtime.last_triggers.clear();
            std::mem::take(&mut runtime.registered)
        };
        for shortcut in registered {
            let _ = app.global_shortcut().unregister(shortcut);
        }
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn load_global_utility_preferences(
    owner_user_id: String,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, GlobalUtilityPreferencesStore>,
) -> Result<GlobalUtilityPreferencesSnapshot, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let worker = store.inner().clone();
    let file = tauri::async_runtime::spawn_blocking(move || worker.state())
        .await
        .map_err(|_| utility_error())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let registrations = store.reconfigure(&app, &owner_user_id, &file)?;
    if let Err(error) = authentication
        .require_session_generation(&owner_user_id, generation)
        .await
    {
        let _ = store.deactivate(&app, &owner_user_id);
        return Err(error);
    }
    Ok(file.snapshot(registrations))
}

#[tauri::command]
pub(crate) async fn update_global_utility_preferences(
    input: UpdateGlobalUtilityPreferencesInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, GlobalUtilityPreferencesStore>,
) -> Result<GlobalUtilityPreferencesSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    validate_update(&input)?;
    let owner_user_id = input.owner_user_id.clone();
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let worker = store.inner().clone();
    let file = tauri::async_runtime::spawn_blocking(move || worker.update(input))
        .await
        .map_err(|_| utility_error())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let registrations = store.reconfigure(&app, &owner_user_id, &file)?;
    if let Err(error) = authentication
        .require_session_generation(&owner_user_id, generation)
        .await
    {
        let _ = store.deactivate(&app, &owner_user_id);
        return Err(error);
    }
    Ok(file.snapshot(registrations))
}

#[tauri::command]
pub(crate) fn deactivate_global_utilities(
    owner_user_id: String,
    app: AppHandle,
    store: State<'_, GlobalUtilityPreferencesStore>,
) -> Result<(), String> {
    validate_owner(&owner_user_id)?;
    store.deactivate(&app, &owner_user_id)
}

fn default_hot_keys() -> BTreeMap<GlobalUtilityAction, GlobalShortcutSpec> {
    BTreeMap::from([
        (
            GlobalUtilityAction::Screenshot,
            GlobalShortcutSpec::new("A", &["control"]),
        ),
        (
            GlobalUtilityAction::ScreenRecording,
            GlobalShortcutSpec::new("Q", &["control"]),
        ),
        (
            GlobalUtilityAction::ClipboardHistory,
            GlobalShortcutSpec::new("E", &["control"]),
        ),
        (
            GlobalUtilityAction::QuickSearch,
            GlobalShortcutSpec::new("Space", &["control"]),
        ),
    ])
}

fn disabled_registration(requested: GlobalShortcutSpec) -> GlobalShortcutRegistration {
    GlobalShortcutRegistration {
        state: GlobalShortcutRegistrationState::Disabled,
        requested,
        active: None,
        uses_fallback: false,
    }
}

fn validate_update(input: &UpdateGlobalUtilityPreferencesInput) -> Result<(), String> {
    if input.expected_revision == 0 {
        return Err("全局工具设置版本无效。".into());
    }
    validate_hot_keys(&input.hot_keys)
}

fn validate_file(file: &GlobalUtilityPreferencesFile) -> Result<(), String> {
    if file.version != FILE_VERSION || file.revision == 0 {
        return Err(utility_error());
    }
    validate_hot_keys(&file.hot_keys).map_err(|_| utility_error())
}

fn validate_hot_keys(
    hot_keys: &BTreeMap<GlobalUtilityAction, GlobalShortcutSpec>,
) -> Result<(), String> {
    if hot_keys.len() != GlobalUtilityAction::ALL.len()
        || GlobalUtilityAction::ALL
            .iter()
            .any(|action| !hot_keys.contains_key(action))
    {
        return Err("全局快捷键配置不完整。".into());
    }
    for shortcut in hot_keys.values() {
        validate_shortcut(shortcut)?;
    }
    Ok(())
}

fn validate_shortcut(shortcut: &GlobalShortcutSpec) -> Result<(), String> {
    let valid_named_key = matches!(
        shortcut.key.as_str(),
        "Space"
            | "Enter"
            | "Escape"
            | "Tab"
            | "ArrowUp"
            | "ArrowDown"
            | "ArrowLeft"
            | "ArrowRight"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "Delete"
            | "Backspace"
            | "Insert"
    );
    let valid_function_key = shortcut
        .key
        .strip_prefix('F')
        .and_then(|value| value.parse::<u8>().ok())
        .is_some_and(|value| (1..=24).contains(&value));
    let valid_single_key = shortcut.key.len() == 1
        && shortcut
            .key
            .as_bytes()
            .first()
            .is_some_and(|value| value.is_ascii_uppercase() || value.is_ascii_digit());
    if !(valid_named_key || valid_function_key || valid_single_key) {
        return Err("快捷键按键不受支持。".into());
    }
    if shortcut.modifiers.is_empty() || shortcut.modifiers.len() > 4 {
        return Err("快捷键必须包含至少一个修饰键。".into());
    }
    let mut unique = BTreeSet::new();
    for modifier in &shortcut.modifiers {
        if !matches!(modifier.as_str(), "control" | "alt" | "shift" | "super")
            || !unique.insert(modifier)
        {
            return Err("快捷键修饰键无效。".into());
        }
    }
    Ok(())
}

fn dispatch_action(app: &AppHandle, action: GlobalUtilityAction) {
    #[cfg(target_os = "windows")]
    if matches!(
        action,
        GlobalUtilityAction::Screenshot | GlobalUtilityAction::ScreenRecording
    ) {
        let uri = if action == GlobalUtilityAction::Screenshot {
            "ms-screenclip:"
        } else {
            "ms-screenclip:?type=recording"
        };
        let _ = std::process::Command::new("explorer.exe").arg(uri).spawn();
    }
    if matches!(
        action,
        GlobalUtilityAction::ClipboardHistory | GlobalUtilityAction::QuickSearch
    ) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    let _ = app.emit(
        "global-utility-triggered",
        GlobalUtilityTriggered { action },
    );
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

fn utility_error() -> String {
    "无法读取或保存全局工具设置。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_macos_behavior_with_windows_modifier_mapping() {
        let defaults = GlobalUtilityPreferencesFile::default();
        assert!(!defaults.is_enabled);
        assert!(defaults.screenshot_enabled);
        assert!(defaults.screen_recording_enabled);
        assert!(defaults.clipboard_history_enabled);
        assert!(defaults.quick_search_enabled);
        assert_eq!(
            defaults.hot_keys[&GlobalUtilityAction::Screenshot].plugin_string(),
            Ok("Ctrl+A".into())
        );
        assert_eq!(
            defaults.hot_keys[&GlobalUtilityAction::QuickSearch].plugin_string(),
            Ok("Ctrl+Space".into())
        );
        assert!(Shortcut::from_str("Ctrl+A").is_ok());
        assert!(Shortcut::from_str("Ctrl+Space").is_ok());
    }

    #[test]
    fn shortcut_validation_rejects_unmodified_unknown_and_duplicate_modifiers() {
        assert!(validate_shortcut(&GlobalShortcutSpec::new("A", &[])).is_err());
        assert!(validate_shortcut(&GlobalShortcutSpec::new("MediaPlay", &["control"])).is_err());
        assert!(validate_shortcut(&GlobalShortcutSpec::new("A", &["control", "control"])).is_err());
    }

    #[test]
    fn file_round_trip_and_revision_conflict_are_enforced() {
        let directory = tempfile::tempdir().unwrap();
        let store = GlobalUtilityPreferencesStore::new(directory.path().into());
        let current = store.state().unwrap();
        let input = UpdateGlobalUtilityPreferencesInput {
            owner_user_id: "owner-1".into(),
            is_enabled: true,
            has_acknowledged_shortcut_conflicts: true,
            screenshot_enabled: true,
            screen_recording_enabled: false,
            clipboard_history_enabled: true,
            quick_search_enabled: true,
            hot_keys: default_hot_keys(),
            expected_revision: current.revision,
        };
        let saved = store.update(input).unwrap();
        assert_eq!(saved.revision, 2);
        assert!(saved.is_enabled);
        assert!(!saved.screen_recording_enabled);
        assert_eq!(store.state().unwrap(), saved);

        let stale = UpdateGlobalUtilityPreferencesInput {
            owner_user_id: "owner-1".into(),
            is_enabled: false,
            has_acknowledged_shortcut_conflicts: true,
            screenshot_enabled: true,
            screen_recording_enabled: true,
            clipboard_history_enabled: true,
            quick_search_enabled: true,
            hot_keys: default_hot_keys(),
            expected_revision: 1,
        };
        assert!(store.update(stale).unwrap_err().contains("已经变化"));
    }
}
